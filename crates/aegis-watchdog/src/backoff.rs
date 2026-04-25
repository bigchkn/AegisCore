use rand::Rng;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct BackoffPolicy {
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub multiplier: f64,
    pub jitter_ratio: f64,
}

impl BackoffPolicy {
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let sample = rand::thread_rng().gen_range(-1.0..=1.0);
        self.delay_for_attempt_with_sample(attempt, sample)
    }

    pub fn delay_for_attempt_with_sample(&self, attempt: u32, sample: f64) -> Duration {
        let clamped_sample = sample.clamp(-1.0, 1.0);
        let base_secs = self.initial_delay.as_secs_f64() * self.multiplier.powi(attempt as i32);
        let capped_secs = base_secs.min(self.max_delay.as_secs_f64());
        let jitter_scale = 1.0 + (self.jitter_ratio * clamped_sample);
        let adjusted_secs = (capped_secs * jitter_scale)
            .max(0.0)
            .min(self.max_delay.as_secs_f64());
        Duration::from_secs_f64(adjusted_secs)
    }
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(5),
            max_delay: Duration::from_secs(300),
            multiplier: 2.0,
            jitter_ratio: 0.2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_increases_and_caps() {
        let policy = BackoffPolicy {
            jitter_ratio: 0.0,
            ..BackoffPolicy::default()
        };

        assert_eq!(
            policy.delay_for_attempt_with_sample(0, 0.0),
            Duration::from_secs(5)
        );
        assert_eq!(
            policy.delay_for_attempt_with_sample(1, 0.0),
            Duration::from_secs(10)
        );
        assert_eq!(
            policy.delay_for_attempt_with_sample(2, 0.0),
            Duration::from_secs(20)
        );
        assert_eq!(
            policy.delay_for_attempt_with_sample(6, 0.0),
            Duration::from_secs(300)
        );
        assert_eq!(
            policy.delay_for_attempt_with_sample(12, 0.0),
            Duration::from_secs(300)
        );
    }

    #[test]
    fn backoff_applies_bounded_jitter() {
        let policy = BackoffPolicy {
            initial_delay: Duration::from_secs(10),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
            jitter_ratio: 0.2,
        };

        assert_eq!(
            policy.delay_for_attempt_with_sample(0, -1.0),
            Duration::from_secs(8)
        );
        assert_eq!(
            policy.delay_for_attempt_with_sample(0, 1.0),
            Duration::from_secs(12)
        );
        assert_eq!(
            policy.delay_for_attempt_with_sample(3, 1.0),
            Duration::from_secs(60)
        );
    }
}
