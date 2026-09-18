use crate::config::{Config, Layout, Theme, WindowStyle};
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

#[derive(Debug, Clone, PartialEq)]
pub struct MetricBlock {
    pub label: String,
    pub kind: MetricKind,
    pub percent: Option<f32>,
}

pub fn metric_blocks(state: &MonitorState, config: &Config) -> Vec<MetricBlock> {
    let mut blocks = Vec::new();
    if config.show_download {
        blocks.push(MetricBlock {
            label: format!("↓ {}", format_rate(state.download_bytes_per_second)),
            kind: MetricKind::Network,
            percent: None,
        });
    }
    if config.show_upload {
        blocks.push(MetricBlock {
            label: format!("↑ {}", format_rate(state.upload_bytes_per_second)),
            kind: MetricKind::Network,
            percent: None,
        });
    }

    if config.show_cpu {
        let text = if config.show_cpu_temperature {
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
        blocks.push(MetricBlock {
            label: text,
            kind: MetricKind::Cpu,
            percent: state.cpu_usage_percent,
        });
    }

    if config.show_memory {
        blocks.push(MetricBlock {
            label: format!("RAM {}", format_percent(state.memory_usage_percent)),
            kind: MetricKind::Memory,
            percent: state.memory_usage_percent,
        });
    }

    if config.show_gpu {
        let text = if config.show_gpu_temperature {
            format_gpu(state.gpu.as_ref(), true)
        } else {
            format_gpu(state.gpu.as_ref(), false)
        };
        blocks.push(MetricBlock {
            label: text,
            kind: MetricKind::Gpu,
            percent: state.gpu.as_ref().and_then(|gpu| gpu.usage_percent),
        });
    }

    blocks
}

pub fn display_lines(state: &MonitorState, config: &Config) -> Vec<DisplayLine> {
    let blocks = metric_blocks(state, config);
    if config.layout == Layout::Horizontal {
        let text = blocks
            .iter()
            .map(|b| b.label.as_str())
            .collect::<Vec<_>>()
            .join("  │  ");
        vec![DisplayLine {
            text,
            kind: MetricKind::Network,
        }]
    } else {
        blocks
            .into_iter()
            .map(|b| DisplayLine {
                text: b.label,
                kind: b.kind,
            })
            .collect()
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
        Theme::Catppuccin => ThemeColors::catppuccin(config.opacity),
        Theme::Cyberpunk => ThemeColors::cyberpunk(config.opacity),
        Theme::Nord => ThemeColors::nord(config.opacity),
        Theme::Matrix => ThemeColors::matrix(config.opacity),
        Theme::Solarized => ThemeColors::solarized(config.opacity),
        Theme::Light => ThemeColors::light(config.opacity),
        Theme::Transparent => ThemeColors::transparent(config.opacity),
        Theme::System if system_dark => ThemeColors::dark(config.opacity),
        Theme::System => ThemeColors::light(config.opacity),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ThemeColors {
    pub background: u32,
    pub border: u32,
    pub foreground: u32,
    pub muted: u32,
    pub separator: u32,
    pub net_accent: u32,
    pub cpu_accent: u32,
    pub ram_accent: u32,
    pub gpu_accent: u32,
    pub bar_track: u32,
    pub normal: u32,
    pub high: u32,
    pub critical: u32,
}

impl ThemeColors {
    pub fn dark(opacity: f32) -> Self {
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
        let border_alpha = (alpha.min(200) * 255) / 255;
        Self {
            background: (alpha << 24) | 0x00_18_1B_22,
            border: (border_alpha << 24) | 0x00_30_36_3D,
            foreground: 0x00_E6_ED_F3,
            muted: 0x00_8B_94_9E,
            separator: 0x00_30_36_3D,
            net_accent: 0x00_58_A6_FF,
            cpu_accent: 0x00_3F_B9_50,
            ram_accent: 0x00_BC_8C_FF,
            gpu_accent: 0x00_D2_99_22,
            bar_track: 0x00_21_26_2D,
            normal: 0x00_3F_B9_50,
            high: 0x00_D2_99_22,
            critical: 0x00_F8_51_49,
        }
    }

    pub fn catppuccin(opacity: f32) -> Self {
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
        Self {
            background: (alpha << 24) | 0x00_24_27_3A,
            border: (alpha << 24) | 0x00_49_4D_64,
            foreground: 0x00_CA_D3_F5,
            muted: 0x00_A5_AD_CB,
            separator: 0x00_36_3A_4F,
            net_accent: 0x00_8A_AD_F4,
            cpu_accent: 0x00_A6_DA_95,
            ram_accent: 0x00_C6_A0_F6,
            gpu_accent: 0x00_F5_A9_7F,
            bar_track: 0x00_36_3A_4F,
            normal: 0x00_A6_DA_95,
            high: 0x00_EE_D4_9F,
            critical: 0x00_ED_87_96,
        }
    }

    pub fn cyberpunk(opacity: f32) -> Self {
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
        Self {
            background: (alpha << 24) | 0x00_0C_09_1A,
            border: (alpha << 24) | 0x00_FF_00_55,
            foreground: 0x00_00_F5_D4,
            muted: 0x00_7B_72_99,
            separator: 0x00_FF_00_55,
            net_accent: 0x00_00_F5_D4,
            cpu_accent: 0x00_FF_E6_00,
            ram_accent: 0x00_FF_00_7F,
            gpu_accent: 0x00_A8_55_F7,
            bar_track: 0x00_21_12_38,
            normal: 0x00_00_F5_D4,
            high: 0x00_FF_E6_00,
            critical: 0x00_FF_00_55,
        }
    }

    pub fn nord(opacity: f32) -> Self {
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
        Self {
            background: (alpha << 24) | 0x00_2E_34_40,
            border: (alpha << 24) | 0x00_4C_56_6A,
            foreground: 0x00_EC_EF_F4,
            muted: 0x00_D8_DE_E9,
            separator: 0x00_43_4C_5E,
            net_accent: 0x00_88_C0_D0,
            cpu_accent: 0x00_A3_BE_8C,
            ram_accent: 0x00_B4_8E_AD,
            gpu_accent: 0x00_81_A1_C1,
            bar_track: 0x00_3B_42_52,
            normal: 0x00_A3_BE_8C,
            high: 0x00_EB_CB_8B,
            critical: 0x00_BF_61_6A,
        }
    }

    pub fn matrix(opacity: f32) -> Self {
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
        Self {
            background: (alpha << 24) | 0x00_05_0E_07,
            border: (alpha << 24) | 0x00_00_AA_44,
            foreground: 0x00_00_FF_66,
            muted: 0x00_15_80_3D,
            separator: 0x00_00_55_22,
            net_accent: 0x00_34_D3_99,
            cpu_accent: 0x00_00_FF_66,
            ram_accent: 0x00_10_B9_81,
            gpu_accent: 0x00_86_EF_AC,
            bar_track: 0x00_09_20_10,
            normal: 0x00_00_FF_66,
            high: 0x00_FA_CC_15,
            critical: 0x00_EF_44_44,
        }
    }

    pub fn solarized(opacity: f32) -> Self {
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
        Self {
            background: (alpha << 24) | 0x00_00_2B_36,
            border: (alpha << 24) | 0x00_07_36_42,
            foreground: 0x00_93_A1_A1,
            muted: 0x00_58_6E_75,
            separator: 0x00_07_36_42,
            net_accent: 0x00_2A_A1_98,
            cpu_accent: 0x00_85_99_00,
            ram_accent: 0x00_26_8B_D2,
            gpu_accent: 0x00_B5_89_00,
            bar_track: 0x00_07_36_42,
            normal: 0x00_85_99_00,
            high: 0x00_CB_4B_16,
            critical: 0x00_DC_32_2F,
        }
    }

    pub fn light(opacity: f32) -> Self {
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
        Self {
            background: (alpha << 24) | 0x00_F8_FA_FC,
            border: (alpha << 24) | 0x00_CB_D5_E1,
            foreground: 0x00_0F_17_2A,
            muted: 0x00_64_74_8B,
            separator: 0x00_CB_D5_E1,
            net_accent: 0x00_02_84_C7,
            cpu_accent: 0x00_16_A3_4A,
            ram_accent: 0x00_7C_3A_ED,
            gpu_accent: 0x00_EA_58_0C,
            bar_track: 0x00_E2_E8_F0,
            normal: 0x00_16_A3_4A,
            high: 0x00_D9_77_06,
            critical: 0x00_DC_26_26,
        }
    }

    pub fn transparent(_opacity: f32) -> Self {
        Self {
            background: 0,
            border: 0,
            foreground: 0x00_F5_F5_F5,
            muted: 0x00_B8_B8_B8,
            separator: 0x00_70_70_70,
            net_accent: 0x00_38_BD_F8,
            cpu_accent: 0x00_4A_DE_80,
            ram_accent: 0x00_C0_84_FC,
            gpu_accent: 0x00_FB_92_3C,
            bar_track: 0x44_00_00_00,
            normal: 0x00_4A_DE_80,
            high: 0x00_FB_BF_24,
            critical: 0x00_F8_71_71,
        }
    }
}

pub fn metric_color(
    kind: MetricKind,
    state: &MonitorState,
    config: &Config,
    colors: ThemeColors,
) -> u32 {
    let base_color = match kind {
        MetricKind::Network => colors.net_accent,
        MetricKind::Cpu => colors.cpu_accent,
        MetricKind::Memory => colors.ram_accent,
        MetricKind::Gpu => colors.gpu_accent,
        MetricKind::Temperature => colors.normal,
    };

    if !config.high_load_colors {
        return base_color;
    }

    match kind {
        MetricKind::Cpu => {
            threshold_color(state.cpu_usage_percent, 70.0, 90.0, base_color, colors)
        }
        MetricKind::Memory => {
            threshold_color(state.memory_usage_percent, 80.0, 92.0, base_color, colors)
        }
        MetricKind::Gpu => threshold_color(
            state.gpu.as_ref().and_then(|gpu| gpu.usage_percent),
            80.0,
            95.0,
            base_color,
            colors,
        ),
        MetricKind::Temperature => {
            threshold_color(state.cpu_temperature_celsius, 75.0, 85.0, base_color, colors)
        }
        MetricKind::Network => base_color,
    }
}

fn threshold_color(
    value: Option<f32>,
    high: f32,
    critical: f32,
    default_color: u32,
    colors: ThemeColors,
) -> u32 {
    match value {
        Some(value) if value >= critical => colors.critical,
        Some(value) if value >= high => colors.high,
        _ => default_color,
    }
}

pub const MENU_ITEM_COUNT: usize = 9;

pub fn calculate_size(
    blocks: &[MetricBlock],
    config: &Config,
    menu_open: bool,
) -> (u32, u32) {
    let font_size = config.font_size;
    let line_height = (font_size * 1.45).ceil();
    let padding_x = match config.window_style {
        WindowStyle::Capsule => (font_size * 1.35).ceil(),
        _ => (font_size * 1.15).ceil(),
    };
    let padding_y = match config.window_style {
        WindowStyle::Capsule => (font_size * 0.55).ceil(),
        _ => (font_size * 0.70).ceil(),
    };
    let mini_bar_w = 34.0;
    let mini_bar_gap = 6.0;

    let (content_w, content_h) = if config.layout == Layout::Horizontal {
        let mut total_w = 0.0;
        let char_width = font_size * 0.62;
        for (i, block) in blocks.iter().enumerate() {
            if i > 0 {
                total_w += char_width * 4.5;
            }
            let block_chars = block.label.chars().count() as f32;
            total_w += block_chars * char_width;
            if (config.window_style == WindowStyle::MiniBar || config.show_mini_bars)
                && block.percent.is_some()
            {
                total_w += mini_bar_gap + mini_bar_w;
            }
        }
        let h = line_height + padding_y * 2.0;
        (total_w + padding_x * 2.0, h)
    } else {
        let mut max_w = 0.0;
        let char_width = font_size * 0.62;
        for block in blocks {
            let mut line_w = block.label.chars().count() as f32 * char_width;
            if (config.window_style == WindowStyle::MiniBar || config.show_mini_bars)
                && block.percent.is_some()
            {
                line_w += mini_bar_gap + mini_bar_w;
            }
            if line_w > max_w {
                max_w = line_w;
            }
        }
        let h = line_height * blocks.len().max(1) as f32 + padding_y * 2.0;
        (max_w + padding_x * 2.0, h)
    };

    let menu_w = if menu_open {
        26.0 * font_size * 0.62 + padding_x * 2.0
    } else {
        0.0
    };
    let menu_h = if menu_open {
        line_height * MENU_ITEM_COUNT as f32 + 8.0
    } else {
        0.0
    };

    let final_w = content_w.max(menu_w).max(120.0).ceil() as u32;
    let final_h = (content_h + menu_h).max(30.0).ceil() as u32;
    (final_w, final_h)
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
    let menu_chars = if menu_open { 26.0 } else { 0.0 };
    let line_height = (font_size * 1.45).ceil();
    let padding_x = (font_size * 1.15).ceil();
    let padding_y = (font_size * 0.70).ceil();
    let width = (max_chars.max(menu_chars) * font_size * 0.63 + padding_x * 2.0).ceil();
    let height = (line_height * lines.len().max(1) as f32
        + padding_y * 2.0
        + if menu_open {
            line_height * MENU_ITEM_COUNT as f32 + 8.0
        } else {
            0.0
        })
    .ceil();
    (width.max(120.0) as u32, height.max(30.0) as u32)
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

    #[test]
    fn theme_palettes_all_constructable() {
        let dark = ThemeColors::dark(0.85);
        assert_ne!(dark.background, 0);
        let catppuccin = ThemeColors::catppuccin(0.85);
        assert_ne!(catppuccin.background, 0);
        let cyberpunk = ThemeColors::cyberpunk(0.85);
        assert_ne!(cyberpunk.border, 0);
        let trans = ThemeColors::transparent(0.0);
        assert_eq!(trans.background, 0);
    }
}
