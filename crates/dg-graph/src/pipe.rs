//! Bounded/unbounded typed packet queues connecting graph elements.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{
    channel, sync_channel, Receiver, RecvTimeoutError, Sender, TryRecvError, TrySendError,
};
use std::sync::Arc;
use std::time::Duration;

use crate::packet::Packet;

/// Default per-edge queue capacity, matching the sophon-stream default.
pub const DEFAULT_QUEUE_CAPACITY: usize = 20;

/// A typed queue carrying packets between two elements.
///
/// Bounded pipes apply backpressure when full; unbounded pipes are used by
/// dataflow-style execution modes where producers run to completion before
/// their consumers start.
pub struct DataPipe {
    sender: PipeSender,
    receiver: PipeReceiver,
}

impl DataPipe {
    pub fn bounded(capacity: usize) -> Self {
        let (sender, receiver) = sync_channel(capacity);
        let state = Arc::new(PipeState::new(Some(capacity)));
        Self {
            sender: PipeSender::Bounded {
                sender,
                state: state.clone(),
            },
            receiver: PipeReceiver { receiver, state },
        }
    }

    pub fn unbounded() -> Self {
        let (sender, receiver) = channel();
        let state = Arc::new(PipeState::new(None));
        Self {
            sender: PipeSender::Unbounded {
                sender,
                state: state.clone(),
            },
            receiver: PipeReceiver { receiver, state },
        }
    }

    pub fn split(self) -> (PipeSender, PipeReceiver) {
        (self.sender, self.receiver)
    }
}

/// Sending half of a [`DataPipe`].
#[derive(Clone)]
pub enum PipeSender {
    Bounded {
        sender: std::sync::mpsc::SyncSender<Packet>,
        state: Arc<PipeState>,
    },
    Unbounded {
        sender: Sender<Packet>,
        state: Arc<PipeState>,
    },
}

impl PipeSender {
    pub fn try_send(&self, packet: Packet) -> std::result::Result<(), TrySendError<Packet>> {
        match self {
            Self::Bounded { sender, state } => {
                state.depth.fetch_add(1, Ordering::Relaxed);
                sender.try_send(packet).inspect_err(|_| {
                    state.depth.fetch_sub(1, Ordering::Relaxed);
                })
            }
            Self::Unbounded { sender, state } => {
                state.depth.fetch_add(1, Ordering::Relaxed);
                sender.send(packet).map_err(|error| {
                    state.depth.fetch_sub(1, Ordering::Relaxed);
                    TrySendError::Disconnected(error.0)
                })
            }
        }
    }

    pub(crate) fn depth(&self) -> usize {
        match self {
            Self::Bounded { state, .. } | Self::Unbounded { state, .. } => state.depth(),
        }
    }
}

/// Receiving half of a [`DataPipe`].
pub struct PipeReceiver {
    receiver: Receiver<Packet>,
    state: Arc<PipeState>,
}

impl PipeReceiver {
    pub fn recv_timeout(&self, timeout: Duration) -> std::result::Result<Packet, RecvTimeoutError> {
        self.receiver.recv_timeout(timeout).inspect(|_| {
            self.state.depth.fetch_sub(1, Ordering::Relaxed);
        })
    }

    pub(crate) fn depth(&self) -> usize {
        self.state.depth.load(Ordering::Relaxed)
    }

    pub(crate) fn try_recv(&self) -> std::result::Result<Packet, TryRecvError> {
        self.receiver.try_recv().inspect(|_| {
            self.state.depth.fetch_sub(1, Ordering::Relaxed);
        })
    }
}

pub struct PipeState {
    depth: AtomicUsize,
    capacity: Option<usize>,
}

impl PipeState {
    fn new(capacity: Option<usize>) -> Self {
        Self {
            depth: AtomicUsize::new(0),
            capacity,
        }
    }

    fn depth(&self) -> usize {
        let depth = self.depth.load(Ordering::Relaxed);
        self.capacity.map_or(depth, |capacity| depth.min(capacity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_recv_and_recv_timeout_decrement_depth_exactly() {
        let pipe = DataPipe::bounded(2);
        let (sender, receiver) = pipe.split();
        sender.try_send(Packet::eos()).unwrap();
        sender.try_send(Packet::eos()).unwrap();
        assert_eq!(sender.depth(), 2);
        assert!(receiver.try_recv().is_ok());
        assert_eq!(receiver.depth(), 1);
        assert!(receiver.recv_timeout(Duration::from_millis(1)).is_ok());
        assert_eq!(receiver.depth(), 0);
    }

    #[test]
    fn disconnect_after_drain_leaves_depth_at_zero() {
        let pipe = DataPipe::bounded(2);
        let (sender, receiver) = pipe.split();
        sender.try_send(Packet::eos()).unwrap();
        assert_eq!(receiver.depth(), 1);
        drop(sender);
        assert!(receiver.try_recv().is_ok());
        assert!(matches!(
            receiver.try_recv(),
            Err(TryRecvError::Disconnected)
        ));
        assert_eq!(receiver.depth(), 0);
    }
}
