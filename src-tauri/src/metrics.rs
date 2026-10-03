use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;

const WINDOW: usize = 512;
#[derive(Default)]
pub struct Metrics {
    ttft: Mutex<VecDeque<f64>>,
    total: Mutex<VecDeque<f64>>,
}
impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push_ttft(&self, value: f64) {
        push(&self.ttft, value);
    }
    pub fn push_total(&self, value: f64) {
        push(&self.total, value);
    }
    pub fn snapshot(&self) -> MetricsSnapshot {
        let ttft = self.ttft.lock().unwrap();
        MetricsSnapshot {
            ttft: stats(&ttft),
            total: stats(&self.total.lock().unwrap()),
            last_ttft: ttft.back().copied(),
        }
    }
}
fn push(samples: &Mutex<VecDeque<f64>>, value: f64) {
    if !value.is_finite() || value < 0.0 {
        return;
    }
    let mut samples = samples.lock().unwrap();
    samples.push_back(value);
    if samples.len() > WINDOW {
        samples.pop_front();
    }
}
#[derive(Debug, Clone, Serialize, Default)]
pub struct Stats {
    pub count: usize,
    pub min: f64,
    pub avg: f64,
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
}
fn stats(v: &VecDeque<f64>) -> Stats {
    if v.is_empty() {
        return Stats::default();
    }
    let mut values: Vec<f64> = v.iter().copied().collect();
    values.sort_by(f64::total_cmp);
    let n = values.len();
    Stats {
        count: n,
        min: values[0],
        max: values[n - 1],
        avg: values.iter().sum::<f64>() / n as f64,
        p50: values[((n - 1) as f64 * 0.5).round() as usize],
        p95: values[((n - 1) as f64 * 0.95).round() as usize],
    }
}
#[derive(Clone, Serialize)]
pub struct MetricsSnapshot {
    pub ttft: Stats,
    pub total: Stats,
    pub last_ttft: Option<f64>,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn percentiles_and_empty_values() {
        let m = Metrics::new();
        assert_eq!(m.snapshot().ttft.count, 0);
        for value in [10.0, 20.0, 30.0, 40.0, 50.0] {
            m.push_ttft(value);
        }
        let s = m.snapshot();
        assert_eq!(s.ttft.p50, 30.0);
        assert_eq!(s.ttft.p95, 50.0);
        assert_eq!(s.ttft.avg, 30.0);
    }
    #[test]
    fn memory_is_bounded_and_nonfinite_values_are_ignored() {
        let m = Metrics::new();
        for i in 0..10000 {
            m.push_total(i as f64);
        }
        m.push_total(f64::NAN);
        m.push_total(-1.0);
        assert_eq!(m.snapshot().total.count, WINDOW);
        assert_eq!(m.snapshot().total.max, 9999.0);
    }
}
