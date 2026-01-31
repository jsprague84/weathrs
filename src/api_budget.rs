use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};

/// Tracks daily API call usage with automatic reset at UTC day boundaries.
pub struct ApiCallBudget {
    daily_limit: u32,
    calls_today: AtomicU32,
    current_day: AtomicI64,
}

impl ApiCallBudget {
    pub fn new(daily_limit: u32) -> Self {
        Self {
            daily_limit,
            calls_today: AtomicU32::new(0),
            current_day: AtomicI64::new(Self::utc_day_now()),
        }
    }

    /// Record an API call. Returns `true` if the call was within budget.
    pub fn record_call(&self) -> bool {
        self.maybe_reset();
        let prev = self.calls_today.fetch_add(1, Ordering::Relaxed);
        prev < self.daily_limit
    }

    /// Number of API calls remaining today.
    pub fn remaining(&self) -> u32 {
        self.maybe_reset();
        let used = self.calls_today.load(Ordering::Relaxed);
        self.daily_limit.saturating_sub(used)
    }

    /// Number of API calls used today.
    pub fn used_today(&self) -> u32 {
        self.maybe_reset();
        self.calls_today.load(Ordering::Relaxed)
    }

    /// Reset counter if the UTC day has changed (compare-and-swap).
    fn maybe_reset(&self) {
        let today = Self::utc_day_now();
        let stored = self.current_day.load(Ordering::Relaxed);
        if today != stored {
            // Attempt to swap the day; only the winner resets the counter.
            if self
                .current_day
                .compare_exchange(stored, today, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                self.calls_today.store(0, Ordering::Relaxed);
            }
        }
    }

    fn utc_day_now() -> i64 {
        chrono::Utc::now().timestamp() / 86400
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_call_within_budget() {
        let budget = ApiCallBudget::new(3);
        assert!(budget.record_call());
        assert!(budget.record_call());
        assert!(budget.record_call());
        // 4th call exceeds budget
        assert!(!budget.record_call());
    }

    #[test]
    fn test_remaining() {
        let budget = ApiCallBudget::new(10);
        assert_eq!(budget.remaining(), 10);
        budget.record_call();
        assert_eq!(budget.remaining(), 9);
    }

    #[test]
    fn test_used_today() {
        let budget = ApiCallBudget::new(100);
        assert_eq!(budget.used_today(), 0);
        budget.record_call();
        budget.record_call();
        assert_eq!(budget.used_today(), 2);
    }
}
