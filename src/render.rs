use crate::config::{Config, Layout, Theme};
use crate::metrics::{
    GpuState, MonitorState, format_bytes, format_percent, format_rate, format_temperature,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricKind {
    Network,
    Cpu,
    Memory,
    Gpu,
    Temperature,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayLine {
    pub text: String,
    pub kind: MetricKind,
}

pub fn display_lines(state: &MonitorState, config: &Config) -> Vec<DisplayLine> {
    let mut network = Vec::new();
    if config.show_download {
        network.push(DisplayLine {
            text: format!("↓ {}", format_rate(state.download_bytes_per_second)),
            kind: MetricKind::Network,
        });
    }
    if config.show_upload {
        network.push(DisplayLine {
            text: format!("↑ {}", format_rate(state.upload_bytes_per_second)),
            kind: MetricKind::Network,
        });
    }

    let mut performance = Vec::new();
    if config.show_cpu {
        let cpu = if config.show_cpu_temperature {
            match (state.cpu_usage_percent, state.cpu_temperature_celsius) {
                (Some(usage), Some(temperature)) => {
                    format!("CPU {usage:.0}% {temperature:.0}°C")
                }
                (Some(usage), None) => format!("CPU {usage:.0}%"),
                (None, Some(temperature)) => format!("CPU N/A {temperature:.0}°C"),
                (None, None) => "CPU N/A".to_string(),
            }
        } else {
            format!("CPU {}", format_percent(state.cpu_usage_percent))
        };
        performance.push(DisplayLine {
            text: cpu,
            kind: MetricKind::Cpu,
        });
    }
    if config.show_memory {
        performance.push(DisplayLine {
            text: format!("RAM {}", format_percent(state.memory_usage_percent)),
            kind: MetricKind::Memory,
        });
    }
    if config.show_gpu {
        if config.show_gpu_temperature {
            performance.push(DisplayLine {
                text: format_gpu(state.gpu.as_ref(), true),
                kind: MetricKind::Gpu,
            });
        } else {
            performance.push(DisplayLine {
                text: format_gpu(state.gpu.as_ref(), false),
                kind: MetricKind::Gpu,
            });
        }
    }

    if config.layout == Layout::Horizontal {
        let mut line = Vec::new();
        line.extend(network);
        line.extend(performance);
        vec![DisplayLine {
            text: line
                .into_iter()
                .map(|item| item.text)
                .collect::<Vec<_>>()
                .join("  │  "),
            kind: MetricKind::Network,
        }]
    } else {
        let mut lines = network;
        lines.extend(performance);
        lines
    }
}

fn format_gpu(gpu: Option<&GpuState>, include_temperature: bool) -> String {
    let Some(gpu) = gpu else {
        return if include_temperature {
            "GPU N/A N/A".to_string()
        } else {
            "GPU N/A".to_string()
        };
    };
    if include_temperature {
        format!(
            "GPU {} {}",
            format_percent(gpu.usage_percent),
            format_temperature(gpu.temperature_celsius)
        )
    } else {
        format!("GPU {}", format_percent(gpu.usage_percent))
    }
}

pub fn theme_colors(config: &Config, system_dark: bool) -> ThemeColors {
    match config.theme {
        Theme::Dark => ThemeColors::dark(config.opacity),
        Theme::Light => ThemeColors::light(config.opacity),
        Theme::Transparent => ThemeColors {
            background: 0,
            foreground: 0x00_F5_F5_F5,
            muted: 0x00_B8_B8_B8,
            separator: 0x00_70_70_70,
            normal: 0x00_7C_DF_9B,
            high: 0x00_F6_C1_77,
            critical: 0x00_F2_7A_7A,
        },
        Theme::System if system_dark => ThemeColors::dark(config.opacity),
        Theme::System => ThemeColors::light(config.opacity),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ThemeColors {
    pub background: u32,
    pub foreground: u32,
    pub muted: u32,
    pub separator: u32,
    pub normal: u32,
    pub high: u32,
    pub critical: u32,
}

impl ThemeColors {
    fn dark(opacity: f32) -> Self {
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
        Self {
            background: (alpha << 24) | 0x00_18_1B_20,
            foreground: 0x00_F5_F7_FA,
            muted: 0x00_AE_B7_C4,
            separator: 0x00_43_4B_57,
            normal: 0x00_7C_DF_9B,
            high: 0x00_F6_C1_77,
            critical: 0x00_F2_7A_7A,
        }
    }

    fn light(opacity: f32) -> Self {
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
        Self {
            background: (alpha << 24) | 0x00_F1_F3_F5,
            foreground: 0x00_1D_24_2C,
            muted: 0x00_5F_6B_7A,
            separator: 0x00_C4_CA_D2,
            normal: 0x00_1B_7F_4B,
            high: 0x00_A3_69_00,
            critical: 0x00_B8_2C_2C,
        }
    }
}

pub fn metric_color(
    kind: MetricKind,
    state: &MonitorState,
    config: &Config,
    colors: ThemeColors,
) -> u32 {
    if !config.high_load_colors {
        return colors.foreground;
    }
    match kind {
        MetricKind::Cpu => threshold_color(state.cpu_usage_percent, 70.0, 90.0, colors),
        MetricKind::Gpu => threshold_color(
            state.gpu.as_ref().and_then(|gpu| gpu.usage_percent),
            80.0,
            95.0,
            colors,
        ),
        MetricKind::Temperature => {
            threshold_color(state.cpu_temperature_celsius, 75.0, 85.0, colors)
        }
        MetricKind::Network | MetricKind::Memory => colors.foreground,
    }
}

fn threshold_color(value: Option<f32>, high: f32, critical: f32, colors: ThemeColors) -> u32 {
    match value {
        Some(value) if value >= critical => colors.critical,
        Some(value) if value >= high => colors.high,
        _ => colors.foreground,
    }
}

pub fn dimensions(
    lines: &[DisplayLine],
    font_size: f32,
    _layout: Layout,
    menu_open: bool,
) -> (u32, u32) {
    let max_chars = lines
        .iter()
        .map(|line| line.text.chars().count())
        .max()
        .unwrap_or(1) as f32;
    let line_height = (font_size * 1.45).ceil();
    let padding_x = (font_size * 1.15).ceil();
    let padding_y = (font_size * 0.75).ceil();
    let width = (max_chars * font_size * 0.63 + padding_x * 2.0).ceil();
    let height = (line_height * lines.len().max(1) as f32
        + padding_y * 2.0
        + if menu_open { line_height * 5.0 } else { 0.0 })
    .ceil();
    (width.max(120.0) as u32, height.max(32.0) as u32)
}

pub fn memory_detail(state: &MonitorState) -> String {
    format!(
        "{} / {}",
        format_bytes(state.memory_used_bytes),
        format_bytes(state.memory_total_bytes)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn horizontal_mode_compacts_all_metrics_into_one_line() {
        let state = MonitorState {
            cpu_usage_percent: Some(32.0),
            cpu_temperature_celsius: Some(58.0),
            memory_usage_percent: Some(47.0),
            download_bytes_per_second: Some(8_200_000.0),
            upload_bytes_per_second: Some(520_000.0),
            gpu: Some(GpuState {
                name: "GPU".to_string(),
                vendor: crate::metrics::GpuVendor::Amd,
                usage_percent: Some(76.0),
                temperature_celsius: Some(67.0),
            }),
            ..MonitorState::default()
        };
        let lines = display_lines(&state, &Config::default());
        assert_eq!(lines.len(), 1);
        assert!(lines[0].text.contains("CPU 32% 58°C"));
        assert!(lines[0].text.contains("RAM 47%"));
    }

    #[test]
    fn unavailable_temperature_is_not_attached_to_cpu_when_disabled() {
        let config = Config {
            show_cpu_temperature: false,
            ..Config::default()
        };
        let lines = display_lines(&MonitorState::default(), &config);
        assert!(lines.iter().any(|line| line.text.contains("CPU N/A")));
    }
}
