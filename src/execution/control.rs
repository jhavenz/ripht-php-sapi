use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::AbortReason;

#[derive(Debug, Default)]
pub struct ExecutionControl {
    claimed: AtomicBool,
    cancelled: AtomicBool,
    client_closed: AtomicBool,
    deadline_exceeded: AtomicBool,
    deadline: Mutex<Option<Instant>>,
}

impl ExecutionControl {
    pub fn with_deadline(deadline: Instant) -> Self {
        Self {
            deadline: Mutex::new(Some(deadline)),
            ..Self::default()
        }
    }

    pub(crate) fn claim_for_request(&self) -> bool {
        self.claimed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub fn cancel(&self) {
        self.cancelled
            .store(true, Ordering::SeqCst);
    }

    pub fn mark_client_closed(&self) {
        self.client_closed
            .store(true, Ordering::SeqCst);
    }

    pub fn set_deadline(&self, deadline: Instant) {
        if let Ok(mut current) = self.deadline.lock() {
            *current = Some(deadline);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
            .load(Ordering::SeqCst)
    }

    pub fn is_client_closed(&self) -> bool {
        self.client_closed
            .load(Ordering::SeqCst)
    }

    pub fn is_deadline_exceeded(&self) -> bool {
        self.deadline_exceeded
            .load(Ordering::SeqCst)
    }

    pub fn deadline_exceeded(&self, now: Instant) -> bool {
        if self.is_deadline_exceeded() {
            return true;
        }

        let Some(deadline) = self
            .deadline
            .lock()
            .ok()
            .and_then(|deadline| *deadline)
        else {
            return false;
        };

        if now < deadline {
            return false;
        }

        self.deadline_exceeded
            .store(true, Ordering::SeqCst);
        true
    }

    pub fn abort_reason(&self) -> Option<AbortReason> {
        if self.is_deadline_exceeded() {
            return Some(AbortReason::DeadlineExceeded);
        }

        if self.is_cancelled() {
            return Some(AbortReason::HostAbort);
        }

        None
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExecutionOptions {
    deadline: Option<Instant>,
    control: Option<Arc<ExecutionControl>>,
}

impl ExecutionOptions {
    pub fn with_control(control: Arc<ExecutionControl>) -> Self {
        Self {
            deadline: None,
            control: Some(control),
        }
    }

    pub fn with_deadline(deadline: Instant) -> Self {
        Self {
            deadline: Some(deadline),
            control: None,
        }
    }

    pub fn deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }

    pub fn control(mut self, control: Arc<ExecutionControl>) -> Self {
        self.control = Some(control);
        self
    }

    pub(crate) fn into_control(self) -> Option<Arc<ExecutionControl>> {
        let control = self
            .control
            .unwrap_or_else(|| Arc::new(ExecutionControl::default()));

        if !control.claim_for_request() {
            return None;
        }

        if let Some(deadline) = self.deadline {
            control.set_deadline(deadline);
        }

        Some(control)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn options_reject_reused_cancelled_control() {
        let control = Arc::new(ExecutionControl::default());

        assert!(ExecutionOptions::with_control(Arc::clone(&control))
            .into_control()
            .is_some());

        control.cancel();

        assert!(ExecutionOptions::with_control(control)
            .into_control()
            .is_none());
    }

    #[test]
    fn options_reject_reused_client_closed_control() {
        let control = Arc::new(ExecutionControl::default());

        assert!(ExecutionOptions::with_control(Arc::clone(&control))
            .into_control()
            .is_some());

        control.mark_client_closed();

        assert!(ExecutionOptions::with_control(control)
            .into_control()
            .is_none());
    }

    #[test]
    fn options_reject_reused_deadline_control() {
        let control = Arc::new(ExecutionControl::with_deadline(
            Instant::now() - Duration::from_secs(1),
        ));

        assert!(ExecutionOptions::with_control(Arc::clone(&control))
            .into_control()
            .is_some());

        assert!(control.deadline_exceeded(Instant::now()));

        assert!(ExecutionOptions::with_control(control)
            .deadline(Instant::now())
            .into_control()
            .is_none());
    }
}
