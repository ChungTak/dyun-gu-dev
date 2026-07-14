//! Generic async pump state machine for avcodec Sans-I/O cores.

use std::collections::VecDeque;

use dg_core::{Error, Result as CoreResult};

type AvResult<T> = core::result::Result<T, dg_media_avcodec::AvError>;

use crate::stats::MediaSessionStats;
use crate::MediaFrame;

/// Maximum queued adapted outputs per core.
pub const MAX_OUTPUT_QUEUE: usize = 8;

/// Core lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreState {
    Accepting,
    Flushing,
    Ended,
    Failed,
}

/// Result of a single pump step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PumpStep {
    Pending,
    OutputReady,
    EndOfStream,
}

/// Submit attempt result preserving backend-owned input on [`SubmitResult::Again`].
pub enum SubmitResult<V> {
    Accepted,
    Again(V),
    Error(dg_media_avcodec::AvError),
}

/// Non-blocking backend operations consumed by [`AsyncPump`].
pub trait BackendOps {
    type BackendValue: Send;

    fn convert_input(&mut self, frame: MediaFrame) -> CoreResult<Self::BackendValue>;
    fn submit_value(&mut self, value: Self::BackendValue) -> SubmitResult<Self::BackendValue>;
    fn poll_output(&mut self) -> AvResult<dg_media_avcodec::Poll<MediaFrame>>;
    fn flush_backend(&mut self) -> AvResult<()>;
    fn reset_backend(&mut self) -> AvResult<()>;
    /// When false the core may complete flush without calling the backend (e.g. encoder never opened).
    fn flush_required(&self) -> bool {
        true
    }
}

/// Shared async pump over a backend that speaks submit/poll/flush.
pub struct AsyncPump<V> {
    state: CoreState,
    pending_media_input: Option<MediaFrame>,
    pending_backend_value: Option<V>,
    output_queue: VecDeque<MediaFrame>,
    flush_sent: bool,
    eos_from_backend: bool,
    error: Option<Error>,
    stats: MediaSessionStats,
}

impl<V> AsyncPump<V> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: CoreState::Accepting,
            pending_media_input: None,
            pending_backend_value: None,
            output_queue: VecDeque::new(),
            flush_sent: false,
            eos_from_backend: false,
            error: None,
            stats: MediaSessionStats::new(),
        }
    }

    #[must_use]
    pub fn stats(&self) -> &MediaSessionStats {
        &self.stats
    }

    #[must_use]
    #[allow(dead_code)] // exercised by unit tests and future driver introspection
    pub const fn state(&self) -> CoreState {
        self.state
    }

    #[must_use]
    pub fn can_accept_input(&self) -> bool {
        self.state == CoreState::Accepting
            && self.pending_media_input.is_none()
            && self.pending_backend_value.is_none()
    }

    #[must_use]
    pub fn has_in_flight(&self) -> bool {
        matches!(self.state, CoreState::Accepting | CoreState::Flushing)
            && (self.pending_media_input.is_some()
                || self.pending_backend_value.is_some()
                || !self.output_queue.is_empty()
                || (self.state == CoreState::Flushing && !self.eos_from_backend))
    }

    #[must_use]
    pub fn is_flushing(&self) -> bool {
        self.state == CoreState::Flushing
    }

    pub fn pop_output(&mut self) -> Option<MediaFrame> {
        self.output_queue.pop_front()
    }

    pub fn submit_input(&mut self, frame: MediaFrame) -> CoreResult<()> {
        if self.state == CoreState::Failed {
            return Err(self
                .error
                .clone()
                .unwrap_or_else(|| Error::Media("avcodec core is in failed state".into())));
        }
        if self.state == CoreState::Ended {
            return Err(Error::Media(
                "avcodec core: input submitted after end of stream".into(),
            ));
        }
        if !self.can_accept_input() {
            return Err(Error::Media(
                "avcodec core: cannot accept input while another input is pending".into(),
            ));
        }
        self.pending_media_input = Some(frame);
        self.stats.submitted = self.stats.submitted.saturating_add(1);
        Ok(())
    }

    pub fn begin_flush(&mut self) {
        if matches!(self.state, CoreState::Accepting) {
            self.state = CoreState::Flushing;
        }
    }

    pub fn pump_step<B: BackendOps<BackendValue = V>>(
        &mut self,
        backend: &mut B,
    ) -> CoreResult<PumpStep> {
        if let Some(error) = self.error.take() {
            self.state = CoreState::Failed;
            return Err(error);
        }
        if self.state == CoreState::Ended {
            return Ok(PumpStep::EndOfStream);
        }
        if self.state == CoreState::Failed {
            return Err(Error::Media("avcodec core is in failed state".into()));
        }

        if let Some(value) = self.pending_backend_value.take() {
            match backend.submit_value(value) {
                SubmitResult::Accepted => {
                    self.stats.accepted = self.stats.accepted.saturating_add(1);
                }
                SubmitResult::Again(value) => {
                    self.pending_backend_value = Some(value);
                    self.stats.again_count = self.stats.again_count.saturating_add(1);
                    self.stats.pending_count = self.stats.pending_count.saturating_add(1);
                    return Ok(PumpStep::Pending);
                }
                SubmitResult::Error(error) if is_end_of_stream(&error) => {
                    return self.fail(Error::Media(
                        "avcodec backend returned end-of-stream during submit before flush".into(),
                    ));
                }
                SubmitResult::Error(error) => return self.fail(map_submit_error(error)),
            }
        } else if let Some(frame) = self.pending_media_input.take() {
            match backend.convert_input(frame) {
                Ok(value) => match backend.submit_value(value) {
                    SubmitResult::Accepted => {
                        self.stats.accepted = self.stats.accepted.saturating_add(1);
                    }
                    SubmitResult::Again(value) => {
                        self.pending_backend_value = Some(value);
                        self.stats.again_count = self.stats.again_count.saturating_add(1);
                        self.stats.pending_count = self.stats.pending_count.saturating_add(1);
                        return Ok(PumpStep::Pending);
                    }
                    SubmitResult::Error(error) if is_end_of_stream(&error) => {
                        return self.fail(Error::Media(
                            "avcodec backend returned end-of-stream during submit before flush"
                                .into(),
                        ));
                    }
                    SubmitResult::Error(error) => return self.fail(map_submit_error(error)),
                },
                Err(error) => return self.fail(error),
            }
        }

        self.after_submit(backend)
    }

    fn after_submit<B: BackendOps<BackendValue = V>>(
        &mut self,
        backend: &mut B,
    ) -> CoreResult<PumpStep> {
        if self.state == CoreState::Flushing
            && !self.flush_sent
            && self.pending_media_input.is_none()
            && self.pending_backend_value.is_none()
        {
            if backend.flush_required() {
                self.stats.flush_calls = self.stats.flush_calls.saturating_add(1);
                match backend.flush_backend() {
                    Ok(()) => self.flush_sent = true,
                    Err(error) if is_again(&error) => {
                        self.stats.again_count = self.stats.again_count.saturating_add(1);
                        self.stats.pending_count = self.stats.pending_count.saturating_add(1);
                        return Ok(PumpStep::Pending);
                    }
                    Err(error) if is_end_of_stream(&error) => self.flush_sent = true,
                    Err(error) => return self.fail(map_submit_error(error)),
                }
            } else {
                self.flush_sent = true;
                self.eos_from_backend = true;
            }
        }

        if self.state == CoreState::Flushing {
            self.stats.flush_polls = self.stats.flush_polls.saturating_add(1);
        }
        match backend.poll_output() {
            Ok(dg_media_avcodec::Poll::Ready(frame)) => {
                if self.output_queue.len() >= MAX_OUTPUT_QUEUE {
                    return self.fail(Error::Media(format!(
                        "avcodec core output queue exceeded maximum {MAX_OUTPUT_QUEUE}"
                    )));
                }
                self.output_queue.push_back(frame);
                self.stats.output_frames = self.stats.output_frames.saturating_add(1);
                Ok(PumpStep::OutputReady)
            }
            Ok(dg_media_avcodec::Poll::Pending) => {
                self.stats.pending_count = self.stats.pending_count.saturating_add(1);
                Ok(PumpStep::Pending)
            }
            Ok(dg_media_avcodec::Poll::EndOfStream) => {
                self.eos_from_backend = true;
                if self.output_queue.is_empty() {
                    self.state = CoreState::Ended;
                    Ok(PumpStep::EndOfStream)
                } else {
                    self.stats.pending_count = self.stats.pending_count.saturating_add(1);
                    Ok(PumpStep::Pending)
                }
            }
            Err(error) if is_again(&error) => {
                self.stats.again_count = self.stats.again_count.saturating_add(1);
                self.stats.pending_count = self.stats.pending_count.saturating_add(1);
                Ok(PumpStep::Pending)
            }
            Err(error) if is_end_of_stream(&error) => {
                self.eos_from_backend = true;
                if self.output_queue.is_empty() {
                    self.state = CoreState::Ended;
                    Ok(PumpStep::EndOfStream)
                } else {
                    self.stats.pending_count = self.stats.pending_count.saturating_add(1);
                    Ok(PumpStep::Pending)
                }
            }
            Err(error) => self.fail(map_submit_error(error)),
        }
    }

    #[allow(dead_code)] // contract for element hot-reset; covered by unit tests
    pub fn reset<B: BackendOps<BackendValue = V>>(&mut self, backend: &mut B) -> CoreResult<()> {
        self.pending_media_input = None;
        self.pending_backend_value = None;
        self.output_queue.clear();
        self.flush_sent = false;
        self.eos_from_backend = false;
        self.error = None;
        self.state = CoreState::Accepting;
        self.stats = MediaSessionStats::new();
        backend.reset_backend().map_err(map_submit_error)
    }

    fn fail(&mut self, error: Error) -> CoreResult<PumpStep> {
        self.error = Some(error.clone());
        self.state = CoreState::Failed;
        Err(error)
    }
}

fn is_again(error: &dg_media_avcodec::AvError) -> bool {
    error.kind() == dg_media_avcodec::AvErrorKind::Again
}

fn is_end_of_stream(error: &dg_media_avcodec::AvError) -> bool {
    error.kind() == dg_media_avcodec::AvErrorKind::EndOfStream
}

fn map_submit_error(error: dg_media_avcodec::AvError) -> Error {
    crate::avcodec::map_av_error(error)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use dg_media_avcodec::{AvError, Poll};

    use super::{
        AsyncPump, AvResult, BackendOps, CoreResult, CoreState, PumpStep, SubmitResult,
        MAX_OUTPUT_QUEUE,
    };
    use crate::MediaFrame;

    struct ScriptBackend {
        submit_script: VecDeque<AvResult<()>>,
        poll_script: VecDeque<AvResult<Poll<u32>>>,
        flush_script: VecDeque<AvResult<()>>,
        reset_calls: usize,
        submit_calls: usize,
        accepted_inputs: Vec<u32>,
        poll_calls: usize,
        flush_calls: usize,
        next_input_id: u32,
        flush_required: bool,
    }

    impl ScriptBackend {
        fn new(
            submit: Vec<AvResult<()>>,
            poll: Vec<AvResult<Poll<u32>>>,
            flush: Vec<AvResult<()>>,
        ) -> Self {
            Self {
                submit_script: submit.into(),
                poll_script: poll.into(),
                flush_script: flush.into(),
                reset_calls: 0,
                submit_calls: 0,
                accepted_inputs: Vec::new(),
                poll_calls: 0,
                flush_calls: 0,
                next_input_id: 0,
                flush_required: true,
            }
        }

        fn without_flush() -> Self {
            let mut backend = Self::new(vec![], vec![Ok(Poll::EndOfStream)], vec![]);
            backend.flush_required = false;
            backend
        }
    }

    impl BackendOps for ScriptBackend {
        type BackendValue = u32;

        fn convert_input(&mut self, frame: MediaFrame) -> CoreResult<u32> {
            let id = frame
                .meta
                .pts
                .and_then(|pts| u32::try_from(pts).ok())
                .unwrap_or(self.next_input_id);
            self.next_input_id = self.next_input_id.saturating_add(1);
            Ok(id)
        }

        fn submit_value(&mut self, value: u32) -> SubmitResult<u32> {
            self.submit_calls += 1;
            match self.submit_script.pop_front().unwrap_or(Ok(())) {
                Ok(()) => {
                    self.accepted_inputs.push(value);
                    SubmitResult::Accepted
                }
                Err(AvError::Again) => SubmitResult::Again(value),
                Err(error) => SubmitResult::Error(error),
            }
        }

        fn poll_output(&mut self) -> AvResult<Poll<MediaFrame>> {
            self.poll_calls += 1;
            let result = self.poll_script.pop_front().unwrap_or(Ok(Poll::Pending));
            match result {
                Ok(Poll::Ready(id)) => {
                    let frame = MediaFrame::from_host_bytes(
                        crate::MediaFrameKind::Image,
                        dg_core::DataType::U8,
                        dg_core::DataFormat::N,
                        vec![1],
                        dg_core::DeviceKind::Cpu,
                        vec![u8::try_from(id).unwrap_or(0)],
                    )
                    .expect("frame");
                    Ok(Poll::Ready(frame))
                }
                Ok(Poll::Pending) => Ok(Poll::Pending),
                Ok(Poll::EndOfStream) => Ok(Poll::EndOfStream),
                Err(error) => Err(error),
            }
        }

        fn flush_backend(&mut self) -> AvResult<()> {
            self.flush_calls += 1;
            self.flush_script.pop_front().unwrap_or(Ok(()))
        }

        fn reset_backend(&mut self) -> AvResult<()> {
            self.reset_calls += 1;
            Ok(())
        }

        fn flush_required(&self) -> bool {
            self.flush_required
        }
    }

    fn frame_with_id(id: i64) -> MediaFrame {
        let mut frame = MediaFrame::from_host_bytes(
            crate::MediaFrameKind::Image,
            dg_core::DataType::U8,
            dg_core::DataFormat::N,
            vec![1],
            dg_core::DeviceKind::Cpu,
            vec![1],
        )
        .expect("frame");
        frame.meta.pts = Some(id);
        frame
    }

    #[test]
    fn submit_again_then_poll_ready_accepts_input_once() {
        let mut backend = ScriptBackend::new(
            vec![Err(AvError::Again), Ok(())],
            vec![Ok(Poll::Ready(7)), Ok(Poll::Pending)],
            vec![],
        );
        let mut pump = AsyncPump::new();
        pump.submit_input(frame_with_id(1)).expect("submit");
        assert_eq!(pump.stats().submitted, 1);
        assert_eq!(pump.pump_step(&mut backend), Ok(PumpStep::Pending));
        assert_eq!(backend.submit_calls, 1);
        assert_eq!(pump.stats().again_count, 1);
        assert_eq!(pump.pump_step(&mut backend), Ok(PumpStep::OutputReady));
        assert_eq!(backend.submit_calls, 2);
        assert_eq!(backend.accepted_inputs, vec![1]);
        assert_eq!(backend.poll_calls, 1);
        assert_eq!(pump.stats().accepted, 1);
        assert_eq!(pump.stats().output_frames, 1);
    }

    #[test]
    fn second_input_waits_until_first_is_accepted() {
        let mut backend = ScriptBackend::new(
            vec![Err(AvError::Again), Ok(())],
            vec![Ok(Poll::Pending)],
            vec![],
        );
        let mut pump = AsyncPump::new();
        pump.submit_input(frame_with_id(1)).expect("first");
        assert!(!pump.can_accept_input());
        assert!(pump.submit_input(frame_with_id(2)).is_err());
        assert_eq!(pump.pump_step(&mut backend), Ok(PumpStep::Pending));
        pump.pump_step(&mut backend).expect("accept first input");
        assert!(pump.can_accept_input());
        pump.submit_input(frame_with_id(2)).expect("second");
    }

    #[test]
    fn flush_waits_for_pending_input_before_flush_call() {
        let mut backend = ScriptBackend::new(
            vec![Err(AvError::Again), Ok(())],
            vec![Ok(Poll::Pending), Ok(Poll::EndOfStream)],
            vec![Ok(())],
        );
        let mut pump = AsyncPump::new();
        pump.submit_input(frame_with_id(1)).expect("submit");
        pump.begin_flush();
        assert!(pump.is_flushing());
        pump.pump_step(&mut backend).expect("drain pending submit");
        assert_eq!(backend.flush_calls, 0);
        pump.pump_step(&mut backend).expect("flush");
        assert_eq!(backend.flush_calls, 1);
        assert_eq!(pump.pump_step(&mut backend), Ok(PumpStep::EndOfStream));
    }

    #[test]
    fn flush_again_then_outputs_then_eos() {
        let mut backend = ScriptBackend::new(
            vec![Ok(())],
            vec![
                Ok(Poll::Pending),
                Ok(Poll::Ready(1)),
                Ok(Poll::Ready(2)),
                Ok(Poll::Pending),
                Ok(Poll::EndOfStream),
            ],
            vec![Err(AvError::Again), Ok(())],
        );
        let mut pump = AsyncPump::new();
        pump.submit_input(frame_with_id(1)).expect("submit");
        pump.pump_step(&mut backend).expect("accept");
        pump.begin_flush();
        pump.pump_step(&mut backend).expect("flush again");
        assert_eq!(backend.flush_calls, 1);
        pump.pump_step(&mut backend).expect("output 1");
        pump.pump_step(&mut backend).expect("output 2");
        assert!(!pump.output_queue.is_empty());
        assert_eq!(pump.pump_step(&mut backend), Ok(PumpStep::Pending));
        pump.pop_output();
        pump.pop_output();
        assert_eq!(pump.pump_step(&mut backend), Ok(PumpStep::EndOfStream));
    }

    #[test]
    fn pending_during_flush_stays_pending_not_eos() {
        let mut backend = ScriptBackend::new(vec![Ok(())], vec![Ok(Poll::Pending)], vec![Ok(())]);
        let mut pump = AsyncPump::new();
        pump.submit_input(frame_with_id(1)).expect("submit");
        pump.pump_step(&mut backend).expect("accept");
        pump.begin_flush();
        pump.pump_step(&mut backend).expect("flush");
        assert_eq!(pump.pump_step(&mut backend), Ok(PumpStep::Pending));
        assert!(pump.is_flushing());
        assert!(!pump.eos_from_backend);
    }

    #[test]
    fn encoder_without_frames_can_end_without_backend_flush() {
        let mut backend = ScriptBackend::without_flush();
        let mut pump = AsyncPump::new();
        pump.begin_flush();
        assert_eq!(pump.pump_step(&mut backend), Ok(PumpStep::EndOfStream));
        assert_eq!(backend.flush_calls, 0);
    }

    #[test]
    fn reset_clears_state_and_allows_rerun() {
        let mut backend = ScriptBackend::new(
            vec![Ok(())],
            vec![Ok(Poll::Ready(3)), Ok(Poll::EndOfStream)],
            vec![Ok(())],
        );
        let mut pump = AsyncPump::new();
        pump.submit_input(frame_with_id(1)).expect("submit");
        pump.begin_flush();
        pump.pump_step(&mut backend).expect("accept");
        pump.pump_step(&mut backend).expect("output");
        pump.reset(&mut backend).expect("reset");
        assert_eq!(pump.state(), CoreState::Accepting);
        assert_eq!(backend.reset_calls, 1);
        pump.submit_input(frame_with_id(9)).expect("submit again");
        pump.pump_step(&mut backend).expect("accept again");
    }

    #[test]
    fn output_queue_has_fixed_upper_bound() {
        assert_eq!(MAX_OUTPUT_QUEUE, 8);
    }
}
