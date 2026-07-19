use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, TrySendError};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use dg_core::{
    Classification, DataType, Detection, FaceDetection, OcrText, ResourcePolicy, Tensor, Track,
};

use crate::error::{Error, Result};
use crate::metrics::ElementMetrics;
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
    max_packets: usize,
    max_bytes: usize,
    current_bytes: usize,
}

impl SinkCollector {
    pub(crate) fn set_budget(&mut self, max_packets: usize, max_bytes: usize) {
        self.max_packets = max_packets;
        self.max_bytes = max_bytes;
    }

    pub(crate) fn try_push(&mut self, packet: &Packet) -> Result<()> {
        if self.total_packets() >= self.max_packets {
            return Err(Error::ResourceLimit {
                resource: "sink packet count".to_string(),
                requested: self.total_packets().saturating_add(1),
                limit: self.max_packets,
            });
        }
        let bytes = packet.owned_bytes_estimate()?;
        let new_bytes = self.current_bytes.saturating_add(bytes);
        if new_bytes > self.max_bytes {
            return Err(Error::ResourceLimit {
                resource: "sink bytes".to_string(),
                requested: new_bytes,
                limit: self.max_bytes,
            });
        }
        self.current_bytes = new_bytes;
        if let Some(tensor) = packet.tensor_ref() {
            self.tensors.push(tensor.clone());
        } else if let Some(detections) = packet.detections_ref() {
            self.detections.push(detections.to_vec());
        } else if let Some(results) = packet.classifications_ref() {
            self.classifications.push(results.to_vec());
        } else if let Some(results) = packet.faces_ref() {
            self.faces.push(results.to_vec());
        } else if let Some(results) = packet.tracks_ref() {
            self.tracks.push(results.to_vec());
        } else if let Some(results) = packet.ocr_ref() {
            self.ocr.push(results.to_vec());
        } else {
            return Err(Error::Runtime(
                "expected tensor or detections payload".to_string(),
            ));
        }
        Ok(())
    }

    fn total_packets(&self) -> usize {
        self.tensors.len()
            + self.detections.len()
            + self.classifications.len()
            + self.faces.len()
            + self.tracks.len()
            + self.ocr.len()
    }

    /// Removes and returns all collected tensors, updating the byte budget.
    pub(crate) fn drain_tensors(&mut self) -> Vec<Tensor> {
        let drained = std::mem::take(&mut self.tensors);
        for tensor in &drained {
            if let Ok(bytes) = tensor.desc().storage_bytes() {
                self.current_bytes = self.current_bytes.saturating_sub(bytes);
            }
        }
        drained
    }
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
    pub(crate) metrics: Arc<ElementMetrics>,
    pub(crate) packet_starts: RefCell<VecDeque<Instant>>,
    pub(crate) max_packet_starts: usize,
    pub(crate) policy: Arc<ResourcePolicy>,
}

impl ElementIo {
    pub fn policy(&self) -> &ResourcePolicy {
        &self.policy
    }

    pub fn should_stop(&self) -> bool {
        self.stop.load(Ordering::Relaxed) || self.control.stop.load(Ordering::Relaxed)
    }

    /// Marks this element as mid-reconnect / connecting (readiness false).
    pub fn set_reconnecting(&self, value: bool) {
        self.metrics.set_reconnecting(value);
    }

    /// Clears connecting/reconnecting without counting a reconnect (first open).
    pub fn clear_reconnecting(&self) {
        self.metrics.clear_reconnecting();
    }

    /// Records a completed reconnect after the initial connection.
    pub fn record_reconnect(&self) {
        self.metrics.record_reconnect();
    }

    /// Records a frame-local drop (bad payload / conversion) without failing the graph.
    pub fn record_drop(&self) {
        self.metrics.record_drop();
    }

    /// Snapshot of element metrics for diagnostics (e.g. drop counts after frame-local errors).
    pub fn metrics_snapshot(&self) -> crate::metrics::ElementMetricsSnapshot {
        self.metrics.snapshot()
    }

    pub fn recv(&self, port: &str) -> Result<Option<Packet>> {
        let receiver = self.inputs.get(port).ok_or_else(|| Error::UnknownPort {
            node: self.name.clone(),
            port: port.to_string(),
        })?;
        let receiver = receiver
            .lock()
            .map_err(|_| Error::Runtime(format!("receive lock poisoned on {port}")))?;
        let result = receiver.recv_timeout(self.send_backoff);
        self.metrics.record_queue_depth(receiver.depth());
        drop(receiver);
        match result {
            Ok(packet) => {
                if packet.is_eos() {
                    self.eos
                        .lock()
                        .map_err(|_| Error::Runtime("EOS state lock poisoned".to_string()))?
                        .seen = true;
                } else {
                    self.metrics.record_received();
                    self.packet_starts.borrow_mut().push_back(Instant::now());
                    if self.packet_starts.borrow().len() > self.max_packet_starts {
                        return Err(Error::ResourceLimit {
                            resource: format!("{}/packet_starts", self.name),
                            requested: self.packet_starts.borrow().len(),
                            limit: self.max_packet_starts,
                        });
                    }
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
            .map_err(|_| Error::Runtime(format!("send lock poisoned on {port}")))?
            .clone();
        let is_eos = packet.is_eos();
        let is_source = self.inputs.is_empty();
        for sender in senders.iter() {
            loop {
                if self.should_stop() {
                    return Err(Error::NotRunning);
                }
                match sender.try_send(packet.clone()) {
                    Ok(()) => {
                        self.metrics.record_queue_depth(sender.depth());
                        break;
                    }
                    Err(TrySendError::Full(_)) => {
                        self.metrics.record_backpressure();
                        thread::sleep(self.send_backoff);
                    }
                    Err(TrySendError::Disconnected(_)) => {
                        self.metrics.record_drop();
                        self.complete_packet()?;
                        return Err(Error::Runtime(format!(
                            "{} downstream disconnected on {port}",
                            self.name
                        )));
                    }
                }
            }
        }
        if !is_eos {
            self.metrics.record_sent();
            if is_source {
                self.metrics.record_source_packet();
            } else {
                self.complete_packet()?;
            }
        }
        Ok(())
    }

    pub fn finish_packet(&self) -> Result<()> {
        self.complete_packet()
    }

    pub fn drop_packet(&self) -> Result<()> {
        self.metrics.record_drop();
        self.complete_packet()
    }

    fn complete_packet(&self) -> Result<()> {
        if let Some(started) = self.packet_starts.borrow_mut().pop_front() {
            self.metrics.record_latency(started.elapsed());
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
