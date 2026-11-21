//! Per-request lifetime and cooperative cancellation for future providers.
//! A cancellation flag/deadline is checked before and after each provider call.
//! Provider implementations doing blocking I/O MUST check during their own work
//! and configure transport-level connection/read timeouts: this does not forcibly
//! preempt a blocked OS call.
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const DEFAULT_TIMEOUT_MS: u64 = 8_000;
pub const MAX_TIMEOUT_MS: u64 = 30_000;
const MIN_TIMEOUT_MS: u64 = 50;
const MAX_ACTIVE: usize = 32;

#[derive(Clone)]
pub struct RequestContext {
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
}

impl RequestContext {
    pub fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("provider_cancelled: Request cancelled".into());
        }
        if Instant::now() >= self.deadline {
            return Err("provider_timeout: Request deadline exceeded".into());
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct RequestRegistry {
    active: Mutex<HashMap<u64, Arc<AtomicBool>>>,
}

impl RequestRegistry {
    pub fn begin(&self, id: u64, timeout_ms: u64) -> Result<RequestGuard<'_>, String> {
        if id == 0 {
            return Err("invalid_request_id: Request ID must be positive".into());
        }
        if !(MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&timeout_ms) {
            return Err("invalid_timeout: Request timeout out of supported range".into());
        }
        let mut active = self
            .active
            .lock()
            .map_err(|_| "provider_internal: Request lock poisoned".to_string())?;
        if active.contains_key(&id) {
            return Err("duplicate_request_id: Request is already in progress".into());
        }
        if active.len() >= MAX_ACTIVE {
            return Err("provider_busy: Too many active content requests".into());
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        active.insert(id, Arc::clone(&cancelled));
        Ok(RequestGuard {
            registry: self,
            id,
            context: RequestContext {
                deadline: Instant::now() + Duration::from_millis(timeout_ms),
                cancelled,
            },
        })
    }

    pub fn cancel(&self, id: u64) -> bool {
        let Ok(active) = self.active.lock() else {
            return false;
        };
        if let Some(token) = active.get(&id) {
            token.store(true, Ordering::Release);
            return true;
        }
        false
    }
}

pub struct RequestGuard<'a> {
    registry: &'a RequestRegistry,
    id: u64,
    context: RequestContext,
}

impl RequestGuard<'_> {
    pub fn context(&self) -> &RequestContext {
        &self.context
    }
}

impl Drop for RequestGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.registry.active.lock() {
            active.remove(&self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn cancellation_and_cleanup_are_visible_to_the_active_request() {
        let registry = RequestRegistry::default();
        let guard = registry.begin(7, DEFAULT_TIMEOUT_MS).unwrap();
        assert!(registry.cancel(7));
        assert!(guard.context().check().unwrap_err().starts_with("provider_cancelled:"));
        assert!(registry.begin(7, DEFAULT_TIMEOUT_MS).is_err());
        drop(guard);
        assert!(!registry.cancel(7));
        assert!(registry.begin(7, DEFAULT_TIMEOUT_MS).is_ok());
    }

    #[test]
    fn rejects_invalid_timeouts_and_duplicate_ids() {
        let registry = RequestRegistry::default();
        assert!(registry.begin(0, DEFAULT_TIMEOUT_MS).is_err());
        assert!(registry.begin(1, 1).is_err());
        assert!(registry.begin(1, MAX_TIMEOUT_MS + 1).is_err());
        let guard = registry.begin(1, DEFAULT_TIMEOUT_MS).unwrap();
        assert!(registry.begin(1, DEFAULT_TIMEOUT_MS).is_err());
        drop(guard);
    }

    #[test]
    fn deadline_expires_even_without_a_cancellation_signal() {
        let registry = RequestRegistry::default();
        let guard = registry.begin(8, 50).unwrap();
        thread::sleep(Duration::from_millis(65));
        assert!(guard.context().check().unwrap_err().starts_with("provider_timeout:"));
    }
}
