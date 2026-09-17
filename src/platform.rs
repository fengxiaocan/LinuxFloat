use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayBackend {
    X11,
    Wayland,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub backend: DisplayBackend,
    pub desktop: String,
    pub compositor: String,
    pub supports_layer_shell: bool,
}

impl SessionInfo {
    pub fn detect() -> Self {
        let session_type = env::var("XDG_SESSION_TYPE")
            .unwrap_or_default()
            .to_ascii_lowercase();
        let desktop = env::var("XDG_CURRENT_DESKTOP")
            .or_else(|_| env::var("DESKTOP_SESSION"))
            .unwrap_or_default();
        let compositor = env::var("XDG_CURRENT_DESKTOP")
            .or_else(|_| env::var("DESKTOP_SESSION"))
            .unwrap_or_default();
        let backend = match session_type.as_str() {
            "x11" => DisplayBackend::X11,
            "wayland" => DisplayBackend::Wayland,
            _ if env::var_os("WAYLAND_DISPLAY").is_some() => DisplayBackend::Wayland,
            _ if env::var_os("DISPLAY").is_some() => DisplayBackend::X11,
            _ => DisplayBackend::Unknown,
        };
        let desktop_lower = format!("{desktop} {compositor}").to_ascii_lowercase();
        let supports_layer_shell = backend == DisplayBackend::Wayland
            && ["sway", "hyprland", "wayfire", "river", "labwc", "niri"]
                .iter()
                .any(|name| desktop_lower.contains(name));

        Self {
            backend,
            desktop,
            compositor,
            supports_layer_shell,
        }
    }

    pub fn overlay_note(&self) -> &'static str {
        match self.backend {
            DisplayBackend::X11 => "X11：使用无边框窗口和置顶层级。",
            DisplayBackend::Wayland if self.supports_layer_shell => {
                "Wayland：当前合成器支持 Layer Shell；窗口置顶由合成器管理。"
            }
            DisplayBackend::Wayland => {
                "Wayland：当前桌面限制应用主动置顶；可通过桌面窗口规则保持置顶。"
            }
            DisplayBackend::Unknown => "无法识别显示会话，将使用桌面默认窗口行为。",
        }
    }
}

impl Default for SessionInfo {
    fn default() -> Self {
        Self::detect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_sessions_have_a_safe_fallback() {
        let info = SessionInfo {
            backend: DisplayBackend::Unknown,
            desktop: String::new(),
            compositor: String::new(),
            supports_layer_shell: false,
        };
        assert!(info.overlay_note().contains("默认窗口行为"));
    }

    #[test]
    fn wlroots_desktops_are_layer_shell_candidates() {
        let info = SessionInfo {
            backend: DisplayBackend::Wayland,
            desktop: "Hyprland".to_string(),
            compositor: "Hyprland".to_string(),
            supports_layer_shell: true,
        };
        assert!(info.overlay_note().contains("Layer Shell"));
    }
}
