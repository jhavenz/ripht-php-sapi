use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ripht_php_sapi::{AbortReason, ResponseHeader, ResponseSink, SinkResult};

use crate::{HeaderValue, LifecycleEvent};

#[derive(Debug, Clone, Default)]
pub struct RecordingSink {
    events: Arc<Mutex<Vec<LifecycleEvent>>>,
    finished: Arc<AtomicBool>,
}

impl RecordingSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> Vec<LifecycleEvent> {
        self.events
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default()
    }

    pub fn into_events(self) -> Vec<LifecycleEvent> {
        Arc::try_unwrap(self.events)
            .ok()
            .and_then(|events| events.into_inner().ok())
            .unwrap_or_default()
    }
}

impl ResponseSink for RecordingSink {
    fn send_headers(
        &mut self,
        status: u16,
        headers: &[ResponseHeader],
    ) -> SinkResult {
        if let Ok(mut events) = self.events.lock() {
            events.push(LifecycleEvent::Headers {
                status_code: status,
                headers: headers
                    .iter()
                    .map(|header| {
                        HeaderValue::new(header.name(), header.value())
                    })
                    .collect(),
            });
        }

        SinkResult::Continue
    }

    fn write(&mut self, bytes: &[u8]) -> SinkResult {
        if let Ok(mut events) = self.events.lock() {
            events.push(LifecycleEvent::Write {
                bytes: bytes.to_vec(),
            });
        }

        SinkResult::Continue
    }

    fn flush(&mut self) -> SinkResult {
        if let Ok(mut events) = self.events.lock() {
            events.push(LifecycleEvent::Flush);
        }

        SinkResult::Continue
    }

    fn finish(&mut self) -> SinkResult {
        self.finished
            .store(true, Ordering::SeqCst);

        if let Ok(mut events) = self.events.lock() {
            events.push(LifecycleEvent::Finish);
        }

        SinkResult::Continue
    }

    fn abort(&mut self, reason: AbortReason) {
        if let Ok(mut events) = self.events.lock() {
            events.push(LifecycleEvent::Abort {
                reason: format!("{reason:?}"),
            });
        }
    }

    fn is_finished(&self) -> bool {
        self.finished
            .load(Ordering::SeqCst)
    }
}
