use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GpuState {
    pub name: String,
    pub vendor: GpuVendor,
    pub usage_percent: Option<f32>,
    pub temperature_celsius: Option<f32>,
}

impl GpuState {
    pub fn new(name: impl Into<String>, vendor: GpuVendor) -> Self {
        Self {
            name: name.into(),
            vendor,
            usage_percent: None,
            temperature_celsius: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MonitorState {
    pub cpu_usage_percent: Option<f32>,
    pub cpu_temperature_celsius: Option<f32>,
    pub memory_usage_percent: Option<f32>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
    pub download_bytes_per_second: Option<f64>,
    pub upload_bytes_per_second: Option<f64>,
    pub gpu: Option<GpuState>,
    pub all_gpus: Vec<GpuState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTicks {
    pub total: u64,
    pub idle: u64,
}

impl CpuTicks {
    pub fn from_proc_stat_line(line: &str) -> Option<Self> {
        let mut fields = line.split_whitespace();
        if fields.next()? != "cpu" {
            return None;
        }

        let values: Vec<u64> = fields
            .take(10)
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        if values.len() < 4 {
            return None;
        }

        let total = values.iter().sum();
        let idle = values[3].saturating_add(values.get(4).copied().unwrap_or_default());
        Some(Self { total, idle })
    }

    pub fn usage_since(self, previous: Self) -> Option<f32> {
        let total_delta = self.total.checked_sub(previous.total)?;
        let idle_delta = self.idle.saturating_sub(previous.idle);
        if total_delta == 0 {
            return None;
        }
        let busy = total_delta.saturating_sub(idle_delta);
        Some((busy as f64 / total_delta as f64 * 100.0).clamp(0.0, 100.0) as f32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

impl MemoryInfo {
    pub fn usage_percent(self) -> f32 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        let used = self.total_bytes.saturating_sub(self.available_bytes);
        (used as f64 / self.total_bytes as f64 * 100.0).clamp(0.0, 100.0) as f32
    }

    pub fn used_bytes(self) -> u64 {
        self.total_bytes.saturating_sub(self.available_bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkCounters {
    pub interface: String,
    pub receive_bytes: u64,
    pub transmit_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct RateSmoother {
    samples: [f64; 3],
    len: usize,
    next: usize,
}

impl Default for RateSmoother {
    fn default() -> Self {
        Self {
            samples: [0.0; 3],
            len: 0,
            next: 0,
        }
    }
}

impl RateSmoother {
    pub fn push(&mut self, value: f64) -> f64 {
        self.samples[self.next] = value.max(0.0);
        self.next = (self.next + 1) % self.samples.len();
        self.len = (self.len + 1).min(self.samples.len());
        self.samples[..self.len].iter().sum::<f64>() / self.len as f64
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug, Clone)]
pub struct CounterSnapshot {
    pub counters: NetworkCounters,
    pub sampled_at: Instant,
}

impl CounterSnapshot {
    pub fn rate_since(&self, previous: &Self) -> Option<(f64, f64)> {
        if self.counters.interface != previous.counters.interface {
            return None;
        }
        let seconds = self
            .sampled_at
            .duration_since(previous.sampled_at)
            .as_secs_f64();
        if seconds <= f64::EPSILON {
            return None;
        }
        let rx = self
            .counters
            .receive_bytes
            .saturating_sub(previous.counters.receive_bytes) as f64
            / seconds;
        let tx = self
            .counters
            .transmit_bytes
            .saturating_sub(previous.counters.transmit_bytes) as f64
            / seconds;
        Some((rx, tx))
    }
}

pub fn parse_meminfo(contents: &str) -> Option<MemoryInfo> {
    let mut total_kib = None;
    let mut available_kib = None;

    for line in contents.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let Some(value) = rest
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        match key {
            "MemTotal" => total_kib = Some(value),
            "MemAvailable" => available_kib = Some(value),
            _ => {}
        }
        if total_kib.is_some() && available_kib.is_some() {
            break;
        }
    }

    Some(MemoryInfo {
        total_bytes: total_kib?.saturating_mul(1024),
        available_bytes: available_kib?.saturating_mul(1024),
    })
}

pub fn parse_net_dev(contents: &str) -> Vec<NetworkCounters> {
    contents
        .lines()
        .filter_map(|line| {
            let (name, values) = line.split_once(':')?;
            let interface = name.trim().to_string();
            if interface.is_empty() {
                return None;
            }
            let fields: Vec<u64> = values
                .split_whitespace()
                .take(9)
                .map(str::parse)
                .collect::<Result<_, _>>()
                .ok()?;
            if fields.len() < 9 {
                return None;
            }
            Some(NetworkCounters {
                interface,
                receive_bytes: fields[0],
                transmit_bytes: fields[8],
            })
        })
        .collect()
}

pub fn choose_network_counters(
    counters: &[NetworkCounters],
    selected_interface: Option<&str>,
    include_vpn: bool,
) -> Option<NetworkCounters> {
    if let Some(selected) = selected_interface {
        return counters
            .iter()
            .find(|item| item.interface == selected)
            .cloned();
    }

    counters
        .iter()
        .filter(|item| is_usable_interface(&item.interface, include_vpn))
        .max_by_key(|item| item.receive_bytes.saturating_add(item.transmit_bytes))
        .cloned()
}

pub fn is_usable_interface(name: &str, include_vpn: bool) -> bool {
    if name == "lo"
        || name.starts_with("docker")
        || name.starts_with("veth")
        || name.starts_with("virbr")
        || name.starts_with("br-")
    {
        return false;
    }

    if !include_vpn
        && (name.starts_with("tun")
            || name.starts_with("tap")
            || name.starts_with("wg")
            || name.starts_with("tailscale"))
    {
        return false;
    }

    true
}

pub fn format_rate(rate: Option<f64>) -> String {
    let Some(rate) = rate else {
        return "N/A".to_string();
    };
    let rate = rate.max(0.0);
    if rate >= 1_000_000_000.0 {
        format!("{:.1}G", rate / 1_000_000_000.0)
    } else if rate >= 1_000_000.0 {
        format!("{:.1}M", rate / 1_000_000.0)
    } else if rate >= 1_000.0 {
        format!("{:.1}K", rate / 1_000.0)
    } else {
        format!("{rate:.0}B")
    }
}

pub fn format_percent(value: Option<f32>) -> String {
    value
        .map(|value| format!("{value:.0}%"))
        .unwrap_or_else(|| "N/A".to_string())
}

pub fn format_temperature(value: Option<f32>) -> String {
    value
        .map(|value| format!("{value:.0}°C"))
        .unwrap_or_else(|| "N/A".to_string())
}

pub fn format_bytes(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else {
        return "N/A".to_string();
    };
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn sample_is_stale(last: Option<Instant>, now: Instant, interval: Duration) -> bool {
    last.is_some_and(|previous| now.duration_since(previous) > interval.saturating_mul(5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_usage_uses_idle_and_iowait() {
        let before = CpuTicks::from_proc_stat_line("cpu 100 10 20 60 10 0 0 0 0 0").unwrap();
        let after = CpuTicks::from_proc_stat_line("cpu 140 10 30 100 20 0 0 0 0 0").unwrap();
        assert_eq!(before.total, 200);
        assert_eq!(before.idle, 70);
        assert!((after.usage_since(before).unwrap() - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn memory_uses_mem_available() {
        let memory =
            parse_meminfo("MemTotal:       16384 kB\nMemFree: 1024 kB\nMemAvailable: 8192 kB\n")
                .unwrap();
        assert_eq!(memory.total_bytes, 16 * 1024 * 1024);
        assert_eq!(memory.used_bytes(), 8 * 1024 * 1024);
        assert!((memory.usage_percent() - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn network_parser_and_filter_keep_real_interfaces() {
        let counters = parse_net_dev(
            "Inter-| Receive | Transmit\n lo: 1 0 0 0 0 0 0 0 2\n eth0: 100 0 0 0 0 0 0 0 200\n docker0: 1000 0 0 0 0 0 0 0 1000\n",
        );
        assert_eq!(counters.len(), 3);
        assert_eq!(
            choose_network_counters(&counters, None, false)
                .unwrap()
                .interface,
            "eth0"
        );
    }

    #[test]
    fn rates_are_smoothed_over_three_samples() {
        let mut smoother = RateSmoother::default();
        assert_eq!(smoother.push(0.0), 0.0);
        assert_eq!(smoother.push(30.0), 15.0);
        assert_eq!(smoother.push(60.0), 30.0);
        assert_eq!(smoother.push(90.0), 60.0);
    }
}
