use super::ResponseHeader;

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkResult {
    Continue,
    Closed,
    Abort,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortReason {
    ClientClosed,
    SinkFailure,
}

pub trait ResponseSink {
    fn send_headers(
        &mut self,
        status: u16,
        headers: &[ResponseHeader],
    ) -> SinkResult;

    fn write(&mut self, bytes: &[u8]) -> SinkResult;
    fn flush(&mut self) -> SinkResult;
    fn finish(&mut self) -> SinkResult;
    fn abort(&mut self, reason: AbortReason);
    fn is_finished(&self) -> bool;
}
