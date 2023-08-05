use chrono::Duration;
use std::sync::atomic::{AtomicU64, Ordering};

// Limiter instances are thread-safe.
pub struct Limiter {
    pub rate: AtomicU64,
    pub allowance: AtomicU64,
    pub max: AtomicU64,
    pub unit: u64,
    pub last_check: AtomicU64,
}

impl Limiter {
    // New creates a new rate limiter instance
    pub fn new(mut rate: i64, per: Duration) -> Limiter {
        let mut nano = per.num_nanoseconds().unwrap() as u64;
        if nano < 1 {
            nano = Duration::seconds(1).num_nanoseconds().unwrap() as u64;
        }

        if rate < 1 {
            rate = 1;
        }

        let rate = rate as u64;

        Limiter {
            rate: AtomicU64::new(rate),
            allowance: AtomicU64::new(rate * nano),
            max: AtomicU64::new(rate * nano),
            unit: nano,
            last_check: AtomicU64::new(unix_nano()),
        }
    }

    // Update rate updates the allowed rate
    pub fn update_rate(&self, rate: i64) {
        let rate = rate as u64;

        self.rate.store(rate, Ordering::Relaxed);
        self.max.store(rate * self.unit, Ordering::Relaxed);
    }

    // Limit returns true if rate was exceeded
    pub fn limit(&self) -> bool {
        let rate = self.rate.load(Ordering::Relaxed);
        // println!("rate is {rate}");
        // println!("unit is {}", self.unit);

        // Calculate the number of ns that have passed since our last call
        let now = unix_nano();
        // println!("now is {now}");

        let passed = now - self.last_check.swap(now, Ordering::Relaxed);
        // println!("passed is {passed}");

        // Add them to our allowance
        let mut prev = self.allowance.load(Ordering::Relaxed);
        // println!("prev is {prev}");

        let mut curr = prev + (passed * rate);

        loop {
            match self.allowance.compare_exchange_weak(
                prev,
                curr,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => prev = x,
            }

            curr = prev + (passed * rate);
        }

        // println!("curr is {curr}");

        // Ensure our allowance is not over maximum
        let max = self.max.load(Ordering::Relaxed);
        // println!("max is {max}");

        if curr >= max {
            self.allowance.fetch_sub(curr - max, Ordering::Relaxed);
            curr = max;
        }

        // If our allowance is less than one unit, rate-limit!
        if curr < self.unit {
            println!("rate-limit!!!!");
            return true;
        }

        // Not limited, subtract a unit
        self.allowance.fetch_sub(self.unit, Ordering::Relaxed);

        false
    }

    // Undo reverts the last Limit() call, returning consumed allowance
    pub fn undo(&self) {
        let max = self.max.load(Ordering::Relaxed);
        let prev = self.allowance.fetch_add(self.unit, Ordering::Relaxed);

        if prev >= max {
            self.allowance.fetch_add(max - prev, Ordering::Relaxed);
        }
    }
}

// now as unix nanoseconds
fn unix_nano() -> u64 {
    chrono::Utc::now().timestamp_nanos() as u64
}

#[cfg(test)]
mod tests {
    use std::thread::sleep;

    use approx::relative_eq;

    use super::*;

    #[test]
    fn should_limit_low_rates() {
        let mut c = 0;
        let l = Limiter::new(10, chrono::Duration::minutes(1));

        while !l.limit() {
            c += 1;
        }

        assert_eq!(c, 10);
    }

    #[test]
    fn should_limit_high_rates() {
        let mut c = 0;
        let l = Limiter::new(1000, chrono::Duration::seconds(1));

        while !l.limit() {
            c += 1;
        }

        relative_eq!(c as f64, 1000 as f64);
    }

    #[test]
    fn should_increase_allowances() {
        let n = 25;
        let l = Limiter::new(n, Duration::milliseconds(50));

        for i in 0..n {
            assert_eq!(l.limit(), false, "on cycle {}", i)
        }

        assert_eq!(l.limit(), true);

        sleep(Duration::milliseconds(10).to_std().unwrap());
        assert_eq!(l.limit(), false);
    }
}
