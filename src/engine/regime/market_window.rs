//! Rolling window of log prices and depth read by the regime classifier.

use std::collections::VecDeque;

const SECONDS_PER_DAY: f64 = 86_400.0;

struct WindowPoint {
    timestamp: u64,
    log_price: f64,
    quote_depth: f64,
}

pub struct MarketWindow {
    window_secs: u64,
    points: VecDeque<WindowPoint>,
}

impl MarketWindow {
    pub fn new(window_secs: u64) -> Self {
        Self {
            window_secs,
            points: VecDeque::new(),
        }
    }

    pub fn push(&mut self, timestamp: u64, price: f64, quote_depth: f64) {
        self.points.push_back(WindowPoint {
            timestamp,
            log_price: price.ln(),
            quote_depth,
        });
        let horizon = timestamp.saturating_sub(self.window_secs);
        while self.points.len() > 1
            && self
                .points
                .front()
                .is_some_and(|point| point.timestamp < horizon)
        {
            self.points.pop_front();
        }
    }

    /// Log return between the two newest points.
    pub fn last_log_return(&self) -> f64 {
        let mut newest = self.points.iter().rev();
        match (newest.next(), newest.next()) {
            (Some(last), Some(before)) => last.log_price - before.log_price,
            _ => 0.0,
        }
    }

    /// Log change from the oldest to the newest point.
    pub fn trend(&self) -> f64 {
        match (self.points.front(), self.points.back()) {
            (Some(first), Some(last)) => last.log_price - first.log_price,
            _ => 0.0,
        }
    }

    /// Realised volatility scaled to one day: `√(Σr² / Σdt × 86 400)`.
    /// Dividing by elapsed time keeps the estimate honest across feed gaps.
    pub fn daily_volatility(&self) -> f64 {
        let (squared_returns, elapsed) = self.points.iter().zip(self.points.iter().skip(1)).fold(
            (0.0, 0.0),
            |(sum, span), (current, next)| {
                let log_return = next.log_price - current.log_price;
                let held_for = (next.timestamp - current.timestamp) as f64;
                (sum + log_return * log_return, span + held_for)
            },
        );
        if elapsed > 0.0 {
            (squared_returns / elapsed * SECONDS_PER_DAY).sqrt()
        } else {
            0.0
        }
    }

    /// Share of the window's peak depth that has disappeared, `0.0..=1.0`.
    pub fn depth_drop(&self) -> f64 {
        let peak = self
            .points
            .iter()
            .map(|point| point.quote_depth)
            .fold(0.0, f64::max);
        match self.points.back() {
            Some(last) if peak > 0.0 => 1.0 - last.quote_depth / peak,
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MarketWindow;

    #[test]
    fn measures_trend_and_last_return() {
        let mut window = MarketWindow::new(1_000);
        window.push(0, 1.0, 10.0);
        window.push(100, 2.0, 10.0);
        window.push(200, 4.0, 10.0);
        assert!((window.trend() - 4.0_f64.ln()).abs() < 1e-12);
        assert!((window.last_log_return() - 2.0_f64.ln()).abs() < 1e-12);
    }

    #[test]
    fn scales_volatility_to_a_day() {
        let mut window = MarketWindow::new(86_400);
        // One ±1% move per hour for a day gives σ_daily = 1% × √24.
        for hour in 0..=24_u64 {
            let price = if hour % 2 == 0 { 1.0 } else { 1.01 };
            window.push(hour * 3_600, price, 10.0);
        }
        let expected = 1.01_f64.ln() * 24.0_f64.sqrt();
        assert!((window.daily_volatility() - expected).abs() < 1e-9);
    }

    #[test]
    fn reports_depth_lost_from_peak() {
        let mut window = MarketWindow::new(1_000);
        window.push(0, 1.0, 100.0);
        window.push(10, 1.0, 40.0);
        assert!((window.depth_drop() - 0.6).abs() < 1e-12);
    }
}
