use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, TrySendError};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use dg_core::{Classification, DataType, Detection, FaceDetection, OcrText, Tensor, Track};

use crate::error::{Error, Result};
use crate::packet::Packet;
use crate::pipe::{PipeReceiver, PipeSender};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortSchema {
    pub name: &'static str,
    pub dtype: Option<DataType>,
    /// For input ports, whether exactly one incoming connection is required.
    /// This flag is ignored for output ports.
    pub required: bool,
}

#[derive(Clone, Debug, Default)]
pub enum ElementHandle {
    #[default]
    None,
    Input(Arc<Mutex<VecDeque<Tensor>>>),
    Sink(Arc<std::sync::Mutex<SinkCollector>>),
}

#[derive(Clone, Debug, Default)]
pub struct SinkCollector {
    pub tensors: Vec<Tensor>,
    pub detections: Vec<Vec<Detection>>,
    pub classifications: Vec<Vec<Classification>>,
    pub faces: Vec<Vec<FaceDetection>>,
    pub tracks: Vec<Vec<Track>>,
    pub ocr: Vec<Vec<OcrText>>,
}

pub struct CreatedElement {
    pub element: Box<dyn Element>,
    pub handle: ElementHandle,
}

pub trait Element: Send {
    fn run(self: Box<Self>, io: ElementIo) -> Result<()>;
}

pub struct ElementIo {
    pub name: String,
    pub inputs: HashMap<String, Arc<Mutex<PipeReceiver>>>,
    pub outputs: HashMap<String, Arc<Mutex<Vec<PipeSender>>>>,
    pub stop: Arc<AtomicBool>,
    pub(crate) control: Arc<NodeControl>,
    pub send_backoff: Duration,
    pub(crate) eos: Arc<Mutex<EosState>>,
}

impl ElementIo {
    pub fn should_stop(&self) -> bool {
        self.stop.load(Ordering::Relaxed) || self.control.stop.load(Ordering::Relaxed)
    }

    pub fn recv(&self, port: &str) -> Result<Option<Packet>> {
        let receiver = self.inputs.get(port).ok_or_else(|| Error::UnknownPort {
            node: self.name.clone(),
            port: port.to_string(),
        })?;
        let receiver = receiver
            .lock()
            .map_err(|_| Error::Runtime(format!("receive lock poisoned on {port}")))?;
        match receiver.recv_timeout(self.send_backoff) {
            Ok(packet) => {
                if packet.is_eos() {
                    self.eos
                        .lock()
                        .map_err(|_| Error::Runtime("EOS state lock poisoned".to_string()))?
                        .seen = true;
                }
                Ok(Some(packet))
            }
            Err(RecvTimeoutError::Timeout) => {
                if self.should_stop() {
                    return Err(Error::NotRunning);
                }
                let seen = self
                    .eos
                    .lock()
                    .map_err(|_| Error::Runtime("EOS state lock poisoned".to_string()))?
                    .seen;
                if seen {
                    Ok(Some(Packet::eos()))
                } else {
                    Ok(None)
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                if self.should_stop() {
                    return Err(Error::NotRunning);
                }
                let seen = self
                    .eos
                    .lock()
                    .map_err(|_| Error::Runtime("EOS state lock poisoned".to_string()))?
                    .seen;
                if seen {
                    Ok(Some(Packet::eos()))
                } else {
                    Err(Error::Runtime(format!(
                        "receive failed on {port}: disconnected"
                    )))
                }
            }
        }
    }

    pub fn send(&self, port: &str, packet: Packet) -> Result<()> {
        let senders = self.outputs.get(port).ok_or_else(|| Error::UnknownPort {
            node: self.name.clone(),
            port: port.to_string(),
        })?;
        let senders = senders
            .lock()
            .map_err(|_| Error::Runtime(format!("send lock poisoned on {port}")))?;
        for sender in senders.iter() {
            loop {
                if self.should_stop() {
                    return Err(Error::NotRunning);
                }
                match sender.try_send(packet.clone()) {
                    Ok(()) => break,
                    Err(TrySendError::Full(_)) => thread::sleep(self.send_backoff),
                    Err(TrySendError::Disconnected(_)) => {
                        return Err(Error::Runtime(format!("downstream disconnected on {port}")))
                    }
                }
            }
        }
        Ok(())
    }

    pub fn broadcast_eos(&self) -> Result<()> {
        let should_broadcast = {
            let mut eos = self
                .eos
                .lock()
                .map_err(|_| Error::Runtime("EOS state lock poisoned".to_string()))?;
            eos.broadcasts += 1;
            eos.broadcasts == eos.instances
        };
        if !should_broadcast {
            return Ok(());
        }
        let packet = Packet::eos();
        for port in self.outputs.keys() {
            self.send(port, packet.clone())?;
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub(crate) struct NodeControl {
    pub stop: AtomicBool,
}

#[derive(Debug)]
pub(crate) struct EosState {
    pub seen: bool,
    pub broadcasts: usize,
    pub instances: usize,
}
