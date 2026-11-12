//! Rolling time-weighted average price. The newest sample carries no weight
//! until the next one arrives, so a single manipulated print cannot move it.

use std::collections::VecDeque;

struct PriceSample {
    timestamp: u64,
    price: f64,
    quote_volume: f64,
}

pub struct TwapWindow {
    window_secs: u64,
    started_at: Option<u64>,
    samples: VecDeque<PriceSample>,
}

impl TwapWindow {
    pub fn new(window_secs: u64) -> Self {
        Self {
            window_secs,
            started_at: None,
            samples: VecDeque::new(),
        }
    }

    pub fn push(&mut self, timestamp: u64, price: f64, quote_volume: f64) {
        self.started_at.get_or_insert(timestamp);
        self.samples.push_back(PriceSample {
            timestamp,
            price,
            quote_volume,
        });
        let horizon = timestamp.saturating_sub(self.window_secs);
        while self.samples.len() > 1
            && self
                .samples
                .front()
                .is_some_and(|sample| sample.timestamp < horizon)
        {
            self.samples.pop_front();
        }
    }

    /// TWAP over the window; the latest price while only one sample exists.
    pub fn average(&self) -> Option<f64> {
        let (weighted_sum, span) = self.samples.iter().zip(self.samples.iter().skip(1)).fold(
            (0.0, 0.0),
            |(sum, span), (current, next)| {
                let held_for = (next.timestamp - current.timestamp) as f64;
                (sum + current.price * held_for, span + held_for)
            },
        );
        if span > 0.0 {
            Some(weighted_sum / span)
        } else {
            self.samples.back().map(|sample| sample.price)
        }
    }

    pub fn quote_volume(&self) -> f64 {
        self.samples.iter().map(|sample| sample.quote_volume).sum()
    }

    /// True once the window has been filling for its full length.
    pub fn is_warm(&self, now: u64) -> bool {
        self.started_at
            .is_some_and(|start| now.saturating_sub(start) >= self.window_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::TwapWindow;

    #[test]
    fn weights_prices_by_holding_time() {
        let mut window = TwapWindow::new(1_000);
        window.push(0, 1.0, 0.0);
        window.push(300, 2.0, 0.0);
        window.push(400, 9.0, 0.0);
        // 1.0 held 300s, 2.0 held 100s; the fresh 9.0 has no weight yet.
        assert!((window.average().expect("samples") - 1.25).abs() < 1e-12);
    }

    #[test]
    fn evicts_samples_older_than_the_window() {
        let mut window = TwapWindow::new(100);
        window.push(0, 5.0, 1.0);
        window.push(150, 1.0, 1.0);
        window.push(200, 1.0, 1.0);
        assert!((window.average().expect("samples") - 1.0).abs() < 1e-12);
        assert!((window.quote_volume() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn warms_up_after_one_full_window() {
        let mut window = TwapWindow::new(100);
        window.push(50, 1.0, 1.0);
        assert!(!window.is_warm(149));
        assert!(window.is_warm(150));
    }
}
