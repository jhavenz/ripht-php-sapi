use crate::execution::{AbortReason, ResponseHeader, ResponseSink, SinkResult};

#[derive(Debug, Default)]
pub(crate) struct ResponseLifecycle {
    headers_finalized: bool,
    response_finished: bool,
    response_aborted: bool,
    client_closed: bool,
    finalized_early: bool,
    abort_reason: Option<AbortReason>,
}

impl ResponseLifecycle {
    pub(crate) fn headers_finalized(&self) -> bool {
        self.headers_finalized
    }

    pub(crate) fn finalize_headers(&mut self) -> bool {
        if self.headers_finalized {
            return false;
        }

        self.headers_finalized = true;
        true
    }

    pub(crate) fn can_write(&self) -> bool {
        !self.response_finished && !self.response_aborted && !self.client_closed
    }

    pub(crate) fn can_flush(&self) -> bool {
        !self.response_finished && !self.response_aborted && !self.client_closed
    }

    pub(crate) fn finish(&mut self) -> bool {
        self.finish_with_origin(false)
    }

    pub(crate) fn finish_early(&mut self) -> bool {
        self.finish_with_origin(true)
    }

    fn finish_with_origin(&mut self, finalized_early: bool) -> bool {
        if !self.can_finish() {
            return false;
        }

        self.response_finished = true;
        self.finalized_early = finalized_early;
        true
    }

    pub(crate) fn can_finish(&self) -> bool {
        !self.response_finished && !self.response_aborted && !self.client_closed
    }

    pub(crate) fn abort(&mut self, reason: AbortReason) -> bool {
        if self.response_finished || self.response_aborted || self.client_closed
        {
            return false;
        }

        self.response_aborted = true;
        self.abort_reason = Some(reason);
        true
    }

    pub(crate) fn mark_client_closed(&mut self) -> bool {
        if self.response_finished || self.response_aborted || self.client_closed
        {
            return false;
        }

        self.client_closed = true;
        true
    }

    pub(crate) fn finalized_early(&self) -> bool {
        self.finalized_early
    }

    pub(crate) fn aborted(&self) -> bool {
        self.response_aborted
    }

    pub(crate) fn client_closed(&self) -> bool {
        self.client_closed
    }

    pub(crate) fn abort_reason(&self) -> Option<AbortReason> {
        self.abort_reason
    }

    #[cfg(test)]
    pub(crate) fn is_finished(&self) -> bool {
        self.response_finished
    }
}

pub struct BufferedResponseSink {
    output: Vec<u8>,
    finalized_output: Vec<u8>,
    finished: bool,
    aborted: bool,
}

impl BufferedResponseSink {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            output: Vec::with_capacity(capacity),
            finalized_output: Vec::new(),
            finished: false,
            aborted: false,
        }
    }

    pub fn capacity(&self) -> usize {
        self.output.capacity()
    }

    pub fn len(&self) -> usize {
        self.output.len()
    }

    pub fn is_empty(&self) -> bool {
        self.output.is_empty()
    }

    pub fn reserve(&mut self, additional: usize) {
        self.output
            .reserve(additional);
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        if self.finished {
            std::mem::take(&mut self.finalized_output)
        } else {
            std::mem::take(&mut self.output)
        }
    }
}

pub struct StreamingResponseSink<F>
where
    F: FnMut(&[u8]),
{
    output: F,
    finished: bool,
    aborted: bool,
}

impl<F> StreamingResponseSink<F>
where
    F: FnMut(&[u8]),
{
    pub fn new(output: F) -> Self {
        Self {
            output,
            finished: false,
            aborted: false,
        }
    }
}

impl<F> ResponseSink for StreamingResponseSink<F>
where
    F: FnMut(&[u8]),
{
    fn send_headers(
        &mut self,
        _status: u16,
        _headers: &[ResponseHeader],
    ) -> SinkResult {
        if self.aborted {
            return SinkResult::Abort;
        }

        SinkResult::Continue
    }

    fn write(&mut self, bytes: &[u8]) -> SinkResult {
        if self.finished {
            return SinkResult::Closed;
        }

        if self.aborted {
            return SinkResult::Abort;
        }

        (self.output)(bytes);
        SinkResult::Continue
    }

    fn flush(&mut self) -> SinkResult {
        if self.aborted {
            return SinkResult::Abort;
        }

        SinkResult::Continue
    }

    fn finish(&mut self) -> SinkResult {
        if self.aborted {
            return SinkResult::Abort;
        }

        if self.finished {
            return SinkResult::Closed;
        }

        self.finished = true;
        SinkResult::Continue
    }

    fn abort(&mut self, _reason: AbortReason) {
        if self.finished {
            return;
        }

        self.aborted = true;
    }

    fn is_finished(&self) -> bool {
        self.finished
    }
}

impl ResponseSink for BufferedResponseSink {
    fn send_headers(
        &mut self,
        _status: u16,
        _headers: &[ResponseHeader],
    ) -> SinkResult {
        if self.aborted {
            return SinkResult::Abort;
        }

        SinkResult::Continue
    }

    fn write(&mut self, bytes: &[u8]) -> SinkResult {
        if self.finished {
            return SinkResult::Closed;
        }

        if self.aborted {
            return SinkResult::Abort;
        }

        self.output
            .extend_from_slice(bytes);
        SinkResult::Continue
    }

    fn flush(&mut self) -> SinkResult {
        if self.aborted {
            return SinkResult::Abort;
        }

        SinkResult::Continue
    }

    fn finish(&mut self) -> SinkResult {
        if self.aborted {
            return SinkResult::Abort;
        }

        if self.finished {
            return SinkResult::Closed;
        }

        self.finished = true;
        self.finalized_output = std::mem::take(&mut self.output);
        SinkResult::Continue
    }

    fn abort(&mut self, _reason: AbortReason) {
        if self.finished {
            return;
        }

        self.aborted = true;
        self.output.clear();
    }

    fn is_finished(&self) -> bool {
        self.finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_state_finishes_once() {
        let mut response = ResponseLifecycle::default();

        assert!(response.finish());
        assert!(!response.finish());
        assert!(response.is_finished());
    }

    #[test]
    fn response_state_discards_after_finish() {
        let mut sink = BufferedResponseSink::with_capacity(16);

        assert_eq!(sink.write(b"before"), SinkResult::Continue);
        assert_eq!(sink.finish(), SinkResult::Continue);
        assert_eq!(sink.write(b"after"), SinkResult::Closed);
        assert_eq!(sink.take_output(), b"before");
    }

    #[test]
    fn response_state_abort_prevents_delivery() {
        let mut response = ResponseLifecycle::default();
        let mut sink = BufferedResponseSink::with_capacity(16);

        assert!(response.abort(AbortReason::SinkFailure));
        sink.abort(AbortReason::SinkFailure);
        assert!(!response.finish());
        assert_eq!(sink.write(b"after"), SinkResult::Abort);
        assert_eq!(sink.take_output(), b"");
        assert_eq!(response.abort_reason(), Some(AbortReason::SinkFailure));
    }
}
