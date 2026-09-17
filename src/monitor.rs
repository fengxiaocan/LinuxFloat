use crate::config::Config;
#[cfg(target_os = "linux")]
use crate::metrics::parse_meminfo;
use crate::metrics::{
    CounterSnapshot, CpuTicks, GpuState, GpuVendor, MonitorState, RateSmoother,
    choose_network_counters, parse_net_dev, sample_is_stale,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const TEMPERATURE_INTERVAL: Duration = Duration::from_secs(3);

pub struct Monitor {
    config: Config,
    previous_cpu: Option<CpuTicks>,
    network: NetworkCollector,
    cpu_temperature: TemperatureReader,
    gpu: GpuCollector,
    last_temperature_sample: Option<Instant>,
    last_cpu_temperature: Option<f32>,
    last_gpus: Vec<GpuState>,
    last_sample: Option<Instant>,
}

impl Monitor {
    pub fn new(config: Config) -> Self {
        Self {
            network: NetworkCollector::new(&config),
            cpu_temperature: TemperatureReader::discover(),
            gpu: GpuCollector::discover(),
            config,
            previous_cpu: None,
            last_temperature_sample: None,
            last_cpu_temperature: None,
            last_gpus: Vec::new(),
            last_sample: None,
        }
    }

    pub fn sample(&mut self) -> MonitorState {
        let now = Instant::now();
        let stale = sample_is_stale(
            self.last_sample,
            now,
            Duration::from_millis(self.config.refresh_interval_ms),
        );
        if stale {
            self.reset_baselines();
        }
        self.last_sample = Some(now);

        let cpu_usage_percent = read_cpu_ticks().and_then(|current| {
            let usage = self
                .previous_cpu
                .and_then(|previous| current.usage_since(previous));
            self.previous_cpu = Some(current);
            usage
        });

        let memory = read_memory();
        let network = self.network.sample(&self.config, now);

        let refresh_temperature = self
            .last_temperature_sample
            .is_none_or(|last| now.duration_since(last) >= TEMPERATURE_INTERVAL);
        if refresh_temperature {
            self.last_temperature_sample = Some(now);
            self.last_cpu_temperature = self.cpu_temperature.read();
            self.last_gpus = self.gpu.sample(true);
        } else {
            self.last_gpus = self.gpu.sample(false);
        }

        let selected_gpu = select_gpu(&self.last_gpus, self.config.gpu.as_deref());
        MonitorState {
            cpu_usage_percent,
            cpu_temperature_celsius: self.last_cpu_temperature,
            memory_usage_percent: memory.map(|value| value.usage_percent()),
            memory_used_bytes: memory.map(|value| value.used_bytes()),
            memory_total_bytes: memory.map(|value| value.total_bytes),
            download_bytes_per_second: network.map(|value| value.0),
            upload_bytes_per_second: network.map(|value| value.1),
            gpu: selected_gpu,
            all_gpus: self.last_gpus.clone(),
        }
    }

    pub fn reset_baselines(&mut self) {
        self.previous_cpu = None;
        self.network.reset();
        self.last_temperature_sample = None;
    }
}

fn select_gpu(gpus: &[GpuState], selected: Option<&str>) -> Option<GpuState> {
    if let Some(selected) = selected
        && let Some(gpu) = gpus
            .iter()
            .find(|gpu| gpu.name == selected || gpu.name.contains(selected))
    {
        return Some(gpu.clone());
    }

    gpus.iter()
        .max_by(|left, right| {
            left.usage_percent
                .unwrap_or_default()
                .partial_cmp(&right.usage_percent.unwrap_or_default())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned()
}

fn read_cpu_ticks() -> Option<CpuTicks> {
    #[cfg(target_os = "linux")]
    {
        let contents = fs::read_to_string("/proc/stat").ok()?;
        contents
            .lines()
            .find(|line| line.starts_with("cpu "))
            .and_then(CpuTicks::from_proc_stat_line)
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn read_memory() -> Option<crate::metrics::MemoryInfo> {
    #[cfg(target_os = "linux")]
    {
        fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|contents| parse_meminfo(&contents))
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

struct NetworkCollector {
    previous: Option<CounterSnapshot>,
    selected_interface: Option<String>,
    known_interfaces: Vec<String>,
    download: RateSmoother,
    upload: RateSmoother,
}

impl NetworkCollector {
    fn new(config: &Config) -> Self {
        Self {
            previous: None,
            selected_interface: config.network_interface.clone(),
            known_interfaces: Vec::new(),
            download: RateSmoother::default(),
            upload: RateSmoother::default(),
        }
    }

    fn sample(&mut self, config: &Config, now: Instant) -> Option<(f64, f64)> {
        #[cfg(target_os = "linux")]
        let contents = fs::read_to_string("/proc/net/dev").ok()?;
        #[cfg(not(target_os = "linux"))]
        let contents = String::new();

        let counters = parse_net_dev(&contents);
        let mut available: Vec<String> =
            counters.iter().map(|item| item.interface.clone()).collect();
        available.sort_unstable();
        if available != self.known_interfaces {
            self.known_interfaces = available;
            self.previous = None;
            self.download.reset();
            self.upload.reset();
        }

        let selected = if let Some(interface) = config.network_interface.as_deref() {
            choose_network_counters(&counters, Some(interface), config.include_vpn)
        } else {
            let cached = self.selected_interface.as_deref().and_then(|name| {
                choose_network_counters(&counters, Some(name), config.include_vpn)
            });
            cached.or_else(|| choose_network_counters(&counters, None, config.include_vpn))
        }?;

        if self.selected_interface.as_deref() != Some(selected.interface.as_str()) {
            self.selected_interface = Some(selected.interface.clone());
            self.previous = None;
            self.download.reset();
            self.upload.reset();
        }

        let current = CounterSnapshot {
            counters: selected,
            sampled_at: now,
        };
        let rates = self
            .previous
            .as_ref()
            .and_then(|previous| current.rate_since(previous));
        self.previous = Some(current);
        rates.map(|(download, upload)| (self.download.push(download), self.upload.push(upload)))
    }

    fn reset(&mut self) {
        self.previous = None;
        self.download.reset();
        self.upload.reset();
    }
}

#[derive(Debug, Clone)]
struct TemperatureSensor {
    input: PathBuf,
}

#[derive(Debug, Clone, Default)]
struct TemperatureReader {
    sensor: Option<TemperatureSensor>,
}

impl TemperatureReader {
    fn discover() -> Self {
        #[cfg(target_os = "linux")]
        {
            Self {
                sensor: discover_cpu_temperature(),
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            Self::default()
        }
    }

    fn read(&mut self) -> Option<f32> {
        if let Some(sensor) = self.sensor.as_ref()
            && let Some(value) = read_temperature(&sensor.input)
        {
            return Some(value);
        }

        *self = Self::discover();
        self.sensor
            .as_ref()
            .and_then(|sensor| read_temperature(&sensor.input))
    }
}

#[cfg(target_os = "linux")]
fn discover_cpu_temperature() -> Option<TemperatureSensor> {
    let mut candidates: Vec<(u8, PathBuf)> = Vec::new();
    let hwmon_root = Path::new("/sys/class/hwmon");
    if let Ok(entries) = fs::read_dir(hwmon_root) {
        for entry in entries.flatten() {
            let directory = entry.path();
            let name = fs::read_to_string(directory.join("name"))
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            if name.contains("amdgpu") || name.contains("nvidia") || name.contains("nouveau") {
                continue;
            }

            for index in 1..=16 {
                let input = directory.join(format!("temp{index}_input"));
                if !input.exists() {
                    continue;
                }
                let label = fs::read_to_string(directory.join(format!("temp{index}_label")))
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase();
                let priority = temperature_priority(&name, &label);
                candidates.push((priority, input));
            }
        }
    }

    if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
        for entry in entries.flatten() {
            let directory = entry.path();
            let type_name = fs::read_to_string(directory.join("type"))
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            if !(type_name.contains("cpu")
                || type_name.contains("pkg")
                || type_name.contains("x86")
                || type_name.contains("core"))
            {
                continue;
            }
            let input = directory.join("temp");
            if input.exists() {
                candidates.push((8, input));
            }
        }
    }

    candidates
        .into_iter()
        .min_by_key(|(priority, _)| *priority)
        .map(|(_, input)| TemperatureSensor { input })
}

#[cfg(target_os = "linux")]
fn temperature_priority(name: &str, label: &str) -> u8 {
    let text = format!("{name} {label}");
    if text.contains("package") {
        0
    } else if text.contains("tctl") {
        1
    } else if text.contains("tdie") {
        2
    } else if text.contains("cpu") || text.contains("core") || name.contains("coretemp") {
        4
    } else {
        20
    }
}

fn read_temperature(path: &Path) -> Option<f32> {
    let value = fs::read_to_string(path).ok()?.trim().parse::<f32>().ok()?;
    if value.abs() > 1_000.0 {
        Some(value / 1_000.0)
    } else {
        Some(value)
    }
}

#[derive(Debug, Clone)]
struct SysfsGpu {
    name: String,
    vendor: GpuVendor,
    usage_path: Option<PathBuf>,
    temperature_path: Option<PathBuf>,
}

struct GpuCollector {
    sysfs: Vec<SysfsGpu>,
    #[cfg(target_os = "linux")]
    nvml: Option<Nvml>,
    last: HashMap<String, GpuState>,
}

impl GpuCollector {
    fn discover() -> Self {
        Self {
            sysfs: discover_sysfs_gpus(),
            #[cfg(target_os = "linux")]
            nvml: Nvml::load(),
            last: HashMap::new(),
        }
    }

    fn sample(&mut self, include_temperature: bool) -> Vec<GpuState> {
        let mut states = Vec::new();
        for sensor in &self.sysfs {
            let mut state = GpuState::new(sensor.name.clone(), sensor.vendor);
            state.usage_percent = sensor
                .usage_path
                .as_ref()
                .and_then(|path| read_number(path))
                .map(|value| value.clamp(0.0, 100.0));
            if include_temperature {
                state.temperature_celsius = sensor
                    .temperature_path
                    .as_ref()
                    .and_then(|path| read_temperature(path));
            } else {
                state.temperature_celsius = self
                    .last
                    .get(&state.name)
                    .and_then(|item| item.temperature_celsius);
            }
            states.push(state);
        }

        #[cfg(target_os = "linux")]
        if let Some(nvml) = self.nvml.as_mut() {
            let mut nvidia = nvml.sample(include_temperature);
            if !include_temperature {
                for state in &mut nvidia {
                    state.temperature_celsius = self
                        .last
                        .get(&state.name)
                        .and_then(|item| item.temperature_celsius);
                }
            }
            states.retain(|state| state.vendor != GpuVendor::Nvidia);
            states.extend(nvidia);
        }

        self.last = states
            .iter()
            .cloned()
            .map(|state| (state.name.clone(), state))
            .collect();
        states
    }
}

#[cfg(target_os = "linux")]
fn discover_sysfs_gpus() -> Vec<SysfsGpu> {
    let mut sensors = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return sensors;
    };

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let card_name = file_name.to_string_lossy();
        if !is_card_device(&card_name) {
            continue;
        }
        let device = entry.path().join("device");
        let vendor = read_vendor(&device.join("vendor"));
        let name = read_gpu_name(&device, vendor, &card_name);
        let usage_path = [
            device.join("gpu_busy_percent"),
            device.join("gt_busy_percent"),
            device.join("gpu_busy"),
        ]
        .into_iter()
        .find(|path| path.exists());
        let temperature_path = find_gpu_temperature(&device, vendor);
        sensors.push(SysfsGpu {
            name,
            vendor,
            usage_path,
            temperature_path,
        });
    }
    sensors
}

#[cfg(not(target_os = "linux"))]
fn discover_sysfs_gpus() -> Vec<SysfsGpu> {
    Vec::new()
}

#[cfg(target_os = "linux")]
fn is_card_device(name: &str) -> bool {
    name.strip_prefix("card")
        .is_some_and(|suffix| !suffix.contains('-') && suffix.parse::<u32>().is_ok())
}

#[cfg(target_os = "linux")]
fn read_vendor(path: &Path) -> GpuVendor {
    match fs::read_to_string(path).unwrap_or_default().trim() {
        "0x10de" => GpuVendor::Nvidia,
        "0x1002" => GpuVendor::Amd,
        "0x8086" => GpuVendor::Intel,
        _ => GpuVendor::Unknown,
    }
}

#[cfg(target_os = "linux")]
fn read_gpu_name(device: &Path, vendor: GpuVendor, card_name: &str) -> String {
    for file in ["product_name", "product", "name"] {
        if let Ok(value) = fs::read_to_string(device.join(file)) {
            let value = value.trim();
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }
    match vendor {
        GpuVendor::Nvidia => format!("NVIDIA {card_name}"),
        GpuVendor::Amd => format!("AMD {card_name}"),
        GpuVendor::Intel => format!("Intel {card_name}"),
        GpuVendor::Unknown => card_name.to_string(),
    }
}

#[cfg(target_os = "linux")]
fn find_gpu_temperature(device: &Path, vendor: GpuVendor) -> Option<PathBuf> {
    if vendor == GpuVendor::Nvidia {
        return None;
    }
    let hwmon = device.join("hwmon");
    let entries = fs::read_dir(hwmon).ok()?;
    for entry in entries.flatten() {
        for index in 1..=16 {
            let path = entry.path().join(format!("temp{index}_input"));
            if path.exists() {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn read_number(_path: &Path) -> Option<f32> {
    None
}

#[cfg(target_os = "linux")]
fn read_number(path: &Path) -> Option<f32> {
    fs::read_to_string(path).ok()?.trim().parse::<f32>().ok()
}

#[cfg(target_os = "linux")]
struct Nvml {
    _library: libloading::Library,
    shutdown: unsafe extern "C" fn() -> i32,
    get_count: unsafe extern "C" fn(*mut u32) -> i32,
    get_handle: unsafe extern "C" fn(u32, *mut *mut std::ffi::c_void) -> i32,
    get_utilization: unsafe extern "C" fn(*mut std::ffi::c_void, *mut NvmlUtilization) -> i32,
    get_temperature: unsafe extern "C" fn(*mut std::ffi::c_void, u32, *mut u32) -> i32,
    get_name: Option<unsafe extern "C" fn(*mut std::ffi::c_void, *mut u8, u32) -> i32>,
}

#[cfg(target_os = "linux")]
#[repr(C)]
struct NvmlUtilization {
    gpu: u32,
    memory: u32,
}

#[cfg(target_os = "linux")]
impl Nvml {
    fn load() -> Option<Self> {
        type Init = unsafe extern "C" fn() -> i32;
        type Shutdown = unsafe extern "C" fn() -> i32;
        type GetCount = unsafe extern "C" fn(*mut u32) -> i32;
        type GetHandle = unsafe extern "C" fn(u32, *mut *mut std::ffi::c_void) -> i32;
        type GetUtilization =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut NvmlUtilization) -> i32;
        type GetTemperature = unsafe extern "C" fn(*mut std::ffi::c_void, u32, *mut u32) -> i32;
        type GetName = unsafe extern "C" fn(*mut std::ffi::c_void, *mut u8, u32) -> i32;

        unsafe {
            let library = libloading::Library::new("libnvidia-ml.so.1").ok()?;
            let init: Init = *library.get(b"nvmlInit_v2\0").ok()?;
            let shutdown: Shutdown = *library.get(b"nvmlShutdown\0").ok()?;
            let get_count: GetCount = *library.get(b"nvmlDeviceGetCount_v2\0").ok()?;
            let get_handle: GetHandle = *library.get(b"nvmlDeviceGetHandleByIndex_v2\0").ok()?;
            let get_utilization: GetUtilization =
                *library.get(b"nvmlDeviceGetUtilizationRates\0").ok()?;
            let get_temperature: GetTemperature =
                *library.get(b"nvmlDeviceGetTemperature\0").ok()?;
            let get_name = library
                .get::<GetName>(b"nvmlDeviceGetName\0")
                .ok()
                .map(|symbol| *symbol);
            if init() != 0 {
                return None;
            }
            Some(Self {
                _library: library,
                shutdown,
                get_count,
                get_handle,
                get_utilization,
                get_temperature,
                get_name,
            })
        }
    }

    fn sample(&self, include_temperature: bool) -> Vec<GpuState> {
        let mut count = 0;
        if unsafe { (self.get_count)(&mut count) } != 0 {
            return Vec::new();
        }

        let mut states = Vec::new();
        for index in 0..count {
            let mut handle = std::ptr::null_mut();
            if unsafe { (self.get_handle)(index, &mut handle) } != 0 || handle.is_null() {
                continue;
            }
            let name = self.device_name(handle, index);
            let mut utilization = NvmlUtilization { gpu: 0, memory: 0 };
            let usage_percent = (unsafe { (self.get_utilization)(handle, &mut utilization) } == 0)
                .then_some(utilization.gpu as f32);
            let temperature_celsius = if include_temperature {
                let mut temperature = 0;
                (unsafe { (self.get_temperature)(handle, 0, &mut temperature) } == 0)
                    .then_some(temperature as f32)
            } else {
                None
            };
            states.push(GpuState {
                name,
                vendor: GpuVendor::Nvidia,
                usage_percent,
                temperature_celsius,
            });
        }
        states
    }

    fn device_name(&self, handle: *mut std::ffi::c_void, index: u32) -> String {
        let Some(get_name) = self.get_name else {
            return format!("NVIDIA GPU {index}");
        };
        let mut buffer = [0_u8; 96];
        if unsafe { get_name(handle, buffer.as_mut_ptr(), buffer.len() as u32) } != 0 {
            return format!("NVIDIA GPU {index}");
        }
        let end = buffer
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(buffer.len());
        String::from_utf8_lossy(&buffer[..end]).trim().to_string()
    }
}

#[cfg(target_os = "linux")]
impl Drop for Nvml {
    fn drop(&mut self) {
        unsafe {
            (self.shutdown)();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_selection_prefers_the_gpu_with_real_load() {
        let mut integrated = GpuState::new("integrated", GpuVendor::Amd);
        integrated.usage_percent = Some(8.0);
        let mut discrete = GpuState::new("discrete", GpuVendor::Nvidia);
        discrete.usage_percent = Some(76.0);
        let selected = select_gpu(&[integrated, discrete], None).unwrap();
        assert_eq!(selected.name, "discrete");
    }

    #[test]
    fn selected_gpu_can_match_a_partial_name() {
        let gpu = GpuState::new("NVIDIA RTX 4060", GpuVendor::Nvidia);
        assert_eq!(
            select_gpu(&[gpu], Some("RTX 4060")).unwrap().name,
            "NVIDIA RTX 4060"
        );
    }
}
