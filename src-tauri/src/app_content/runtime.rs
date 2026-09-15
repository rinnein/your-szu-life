use super::model::DAY_MS;
use std::sync::{Arc, Condvar, Mutex};

pub(super) trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

pub(super) struct SystemClock;
impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }
}

pub(super) struct AutomaticSchedule {
    pub next_at: u64,
}
impl AutomaticSchedule {
    pub fn new(started_at: u64) -> Self {
        Self {
            next_at: started_at,
        }
    }

    pub fn take_due(&mut self, now: u64) -> bool {
        if now < self.next_at {
            return false;
        }
        // Keep the process-start phase, skip missed intervals after suspension.
        let intervals = now.saturating_sub(self.next_at) / DAY_MS + 1;
        self.next_at = self
            .next_at
            .saturating_add(intervals.saturating_mul(DAY_MS));
        true
    }
}

struct Flight<T> {
    value: Mutex<Option<T>>,
    completed: Condvar,
}

/// All callers arriving while work is in progress receive that exact round's result.
/// The completed result stays alive for waiters even if another round has already begun.
pub(super) struct SingleFlight<T> {
    active: Mutex<Option<Arc<Flight<T>>>>,
}
impl<T> Default for SingleFlight<T> {
    fn default() -> Self {
        Self {
            active: Mutex::new(None),
        }
    }
}
impl<T: Clone> SingleFlight<T> {
    pub fn run(&self, work: impl FnOnce() -> T) -> T {
        let (flight, leader) = {
            let mut active = self.active.lock().unwrap();
            if let Some(flight) = &*active {
                (flight.clone(), false)
            } else {
                let flight = Arc::new(Flight {
                    value: Mutex::new(None),
                    completed: Condvar::new(),
                });
                *active = Some(flight.clone());
                (flight, true)
            }
        };
        if leader {
            let result = work();
            *flight.value.lock().unwrap() = Some(result.clone());
            *self.active.lock().unwrap() = None;
            flight.completed.notify_all();
            result
        } else {
            let value = flight.value.lock().unwrap();
            flight
                .completed
                .wait_while(value, |value| value.is_none())
                .unwrap()
                .as_ref()
                .expect("completed flight")
                .clone()
        }
    }
}

#[derive(Default)]
pub(super) struct ImageSlots {
    count: Mutex<usize>,
    available: Condvar,
}
pub(super) struct ImageSlot<'a>(&'a ImageSlots);
impl ImageSlots {
    pub fn acquire(&self) -> ImageSlot<'_> {
        let count = self.count.lock().unwrap();
        let mut count = self
            .available
            .wait_while(count, |count| *count >= 4)
            .unwrap();
        *count += 1;
        ImageSlot(self)
    }
}
impl Drop for ImageSlot<'_> {
    fn drop(&mut self) {
        *self.0.count.lock().unwrap() -= 1;
        self.0.available.notify_one();
    }
}
