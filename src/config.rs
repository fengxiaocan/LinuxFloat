use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const CONFIG_DIR: &str = "linux-monitor";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    System,
    Transparent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub refresh_interval_ms: u64,
    pub layout: Layout,
    pub theme: Theme,
    pub opacity: f32,
    pub font_size: f32,
    pub corner_radius: u32,
    pub always_on_top: bool,
    pub mouse_passthrough: bool,
    pub locked: bool,
    pub autostart: bool,
    pub start_visible: bool,
    pub high_load_colors: bool,
    pub show_download: bool,
    pub show_upload: bool,
    pub show_cpu: bool,
    pub show_cpu_temperature: bool,
    pub show_memory: bool,
    pub show_gpu: bool,
    pub show_gpu_temperature: bool,
    pub include_vpn: bool,
    pub network_interface: Option<String>,
    pub gpu: Option<String>,
    pub position: WindowPosition,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            refresh_interval_ms: 1_000,
            layout: Layout::Horizontal,
            theme: Theme::Dark,
            opacity: 0.85,
            font_size: 13.0,
            corner_radius: 8,
            always_on_top: true,
            mouse_passthrough: false,
            locked: false,
            autostart: false,
            start_visible: true,
            high_load_colors: true,
            show_download: true,
            show_upload: true,
            show_cpu: true,
            show_cpu_temperature: true,
            show_memory: true,
            show_gpu: true,
            show_gpu_temperature: true,
            include_vpn: false,
            network_interface: None,
            gpu: None,
            position: WindowPosition::default(),
        }
    }
}

impl Config {
    pub fn validate(&mut self) {
        self.refresh_interval_ms = match self.refresh_interval_ms {
            500 | 1_000 | 2_000 => self.refresh_interval_ms,
            value if value < 750 => 500,
            value if value < 1_500 => 1_000,
            _ => 2_000,
        };
        self.opacity = self.opacity.clamp(0.0, 1.0);
        self.font_size = self.font_size.clamp(9.0, 32.0);
        self.corner_radius = self.corner_radius.min(32);
    }

    pub fn cycle_layout(&mut self) {
        self.layout = match self.layout {
            Layout::Horizontal => Layout::Vertical,
            Layout::Vertical => Layout::Horizontal,
        };
    }

    pub fn cycle_theme(&mut self) {
        self.theme = match self.theme {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Transparent,
            Theme::Transparent => Theme::System,
            Theme::System => Theme::Dark,
        };
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowPosition {
    pub x: Option<i32>,
    pub y: Option<i32>,
}

pub fn config_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("XDG_CONFIG_HOME")
        && !path.trim().is_empty()
    {
        return Some(PathBuf::from(path).join(CONFIG_DIR).join(CONFIG_FILE));
    }

    if let Ok(path) = env::var("HOME")
        && !path.trim().is_empty()
    {
        return Some(
            PathBuf::from(path)
                .join(".config")
                .join(CONFIG_DIR)
                .join(CONFIG_FILE),
        );
    }

    if let Ok(path) = env::var("APPDATA")
        && !path.trim().is_empty()
    {
        return Some(PathBuf::from(path).join(CONFIG_DIR).join(CONFIG_FILE));
    }

    None
}

pub fn load() -> (Config, Option<String>) {
    let Some(path) = config_path() else {
        return (Config::default(), None);
    };

    match fs::read_to_string(&path) {
        Ok(contents) => match toml::from_str::<Config>(&contents) {
            Ok(mut config) => {
                config.validate();
                (config, None)
            }
            Err(error) => (
                Config::default(),
                Some(format!("配置文件无法解析，将使用默认值: {error}")),
            ),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => (Config::default(), None),
        Err(error) => (
            Config::default(),
            Some(format!("无法读取配置文件 {}: {error}", path.display())),
        ),
    }
}

pub fn save(config: &Config) -> io::Result<()> {
    let Some(path) = config_path() else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "无法定位用户配置目录",
        ));
    };

    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "配置文件路径没有父目录"))?;
    fs::create_dir_all(parent)?;

    let contents = toml::to_string_pretty(config)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let temporary = path.with_extension("toml.tmp");
    fs::write(&temporary, contents)?;
    fs::rename(temporary, path)
}

pub fn ensure_parent(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_mvp() {
        let config = Config::default();
        assert_eq!(config.refresh_interval_ms, 1_000);
        assert_eq!(config.layout, Layout::Horizontal);
        assert!((config.opacity - 0.85).abs() < f32::EPSILON);
        assert!(config.always_on_top);
        assert!(!config.mouse_passthrough);
    }

    #[test]
    fn invalid_values_are_clamped_to_supported_choices() {
        let mut config = Config {
            refresh_interval_ms: 25,
            opacity: 4.0,
            font_size: 1.0,
            corner_radius: 100,
            ..Config::default()
        };
        config.validate();
        assert_eq!(config.refresh_interval_ms, 500);
        assert_eq!(config.opacity, 1.0);
        assert_eq!(config.font_size, 9.0);
        assert_eq!(config.corner_radius, 32);
    }
}
