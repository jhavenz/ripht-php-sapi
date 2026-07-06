use ripht_php_sapi::{AbortReason, ResponseHeader, ResponseSink, SinkResult};

use crate::{HeaderValue, LifecycleEvent};

#[derive(Debug, Default)]
pub struct RecordingSink {
    events: Vec<LifecycleEvent>,
    finished: bool,
}

impl RecordingSink {
    pub fn events(&self) -> &[LifecycleEvent] {
        &self.events
    }

    pub fn into_events(self) -> Vec<LifecycleEvent> {
        self.events
    }
}

impl ResponseSink for RecordingSink {
    fn send_headers(
        &mut self,
        status: u16,
        headers: &[ResponseHeader],
    ) -> SinkResult {
        self.events
            .push(LifecycleEvent::Headers {
                status_code: status,
                headers: headers
                    .iter()
                    .map(|header| {
                        HeaderValue::new(header.name(), header.value())
                    })
                    .collect(),
            });

        SinkResult::Continue
    }

    fn write(&mut self, bytes: &[u8]) -> SinkResult {
        self.events
            .push(LifecycleEvent::Write {
                bytes: bytes.to_vec(),
            });

        SinkResult::Continue
    }

    fn flush(&mut self) -> SinkResult {
        self.events
            .push(LifecycleEvent::Flush);

        SinkResult::Continue
    }

    fn finish(&mut self) -> SinkResult {
        self.finished = true;
        self.events
            .push(LifecycleEvent::Finish);

        SinkResult::Continue
    }

    fn abort(&mut self, reason: AbortReason) {
        self.events
            .push(LifecycleEvent::Abort {
                reason: format!("{reason:?}"),
            });
    }

    fn is_finished(&self) -> bool {
        self.finished
    }
}
