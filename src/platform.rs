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

#[cfg(target_os = "linux")]
pub struct X11Context {
    conn: x11rb::rust_connection::RustConnection,
    root: u32,
    net_wm_desktop: u32,
    net_wm_state: u32,
    net_wm_state_sticky: u32,
    net_wm_state_above: u32,
    net_wm_state_skip_taskbar: u32,
    net_wm_state_skip_pager: u32,
}

#[cfg(target_os = "linux")]
impl X11Context {
    pub fn try_new() -> Option<Self> {
        if std::env::var_os("DISPLAY").is_none() {
            return None;
        }
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::ConnectionExt as _;
        let (conn, screen_num) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots.get(screen_num)?.root;

        let net_wm_desktop = conn.intern_atom(false, b"_NET_WM_DESKTOP").ok()?.reply().ok()?.atom;
        let net_wm_state = conn.intern_atom(false, b"_NET_WM_STATE").ok()?.reply().ok()?.atom;
        let net_wm_state_sticky = conn.intern_atom(false, b"_NET_WM_STATE_STICKY").ok()?.reply().ok()?.atom;
        let net_wm_state_above = conn.intern_atom(false, b"_NET_WM_STATE_ABOVE").ok()?.reply().ok()?.atom;
        let net_wm_state_skip_taskbar = conn
            .intern_atom(false, b"_NET_WM_STATE_SKIP_TASKBAR")
            .ok()?
            .reply()
            .ok()?
            .atom;
        let net_wm_state_skip_pager = conn
            .intern_atom(false, b"_NET_WM_STATE_SKIP_PAGER")
            .ok()?
            .reply()
            .ok()?
            .atom;

        Some(Self {
            conn,
            root,
            net_wm_desktop,
            net_wm_state,
            net_wm_state_sticky,
            net_wm_state_above,
            net_wm_state_skip_taskbar,
            net_wm_state_skip_pager,
        })
    }

    pub fn query_pointer(&self) -> Option<(i32, i32)> {
        use x11rb::protocol::xproto::ConnectionExt as _;
        let reply = self.conn.query_pointer(self.root).ok()?.reply().ok()?;
        Some((reply.root_x as i32, reply.root_y as i32))
    }

    pub fn grab_pointer(&self, window: &winit::window::Window) -> bool {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use x11rb::connection::Connection as _;
        use x11rb::protocol::xproto::{ConnectionExt as _, EventMask, GrabMode, GrabStatus};

        let Ok(handle) = window.window_handle() else {
            return false;
        };
        let xid = match handle.as_raw() {
            RawWindowHandle::Xlib(h) => Some(h.window as u32),
            RawWindowHandle::Xcb(h) => Some(h.window.get()),
            _ => None,
        };
        let Some(xid) = xid else {
            return false;
        };

        let mask = EventMask::BUTTON_RELEASE
            | EventMask::BUTTON_PRESS
            | EventMask::POINTER_MOTION
            | EventMask::BUTTON_MOTION;

        if let Ok(cookie) = self.conn.grab_pointer(
            false,
            xid,
            mask,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            x11rb::NONE,
            x11rb::NONE,
            x11rb::CURRENT_TIME,
        ) {
            if let Ok(reply) = cookie.reply() {
                let _ = self.conn.flush();
                return reply.status == GrabStatus::SUCCESS;
            }
        }
        false
    }

    pub fn ungrab_pointer(&self) {
        use x11rb::connection::Connection as _;
        use x11rb::protocol::xproto::ConnectionExt as _;
        let _ = self.conn.ungrab_pointer(x11rb::CURRENT_TIME);
        let _ = self.conn.flush();
    }

    pub fn apply_sticky(&self, window: &winit::window::Window) {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use x11rb::connection::Connection as _;
        use x11rb::protocol::xproto::{
            AtomEnum, ClientMessageData, ClientMessageEvent, ConnectionExt as _, EventMask,
            PropMode,
        };
        use x11rb::wrapper::ConnectionExt as _;

        let Ok(handle) = window.window_handle() else {
            return;
        };
        let xid = match handle.as_raw() {
            RawWindowHandle::Xlib(h) => Some(h.window as u32),
            RawWindowHandle::Xcb(h) => Some(h.window.get()),
            _ => None,
        };
        let Some(xid) = xid else {
            return;
        };

        // 1. Direct window property setting (for unmapped / initial window setup)
        let all_desktops: [u32; 1] = [0xFFFFFFFF];
        let _ = self.conn.change_property32(
            PropMode::REPLACE,
            xid,
            self.net_wm_desktop,
            AtomEnum::CARDINAL,
            &all_desktops,
        );

        let states: [u32; 4] = [
            self.net_wm_state_sticky,
            self.net_wm_state_above,
            self.net_wm_state_skip_taskbar,
            self.net_wm_state_skip_pager,
        ];
        let _ = self.conn.change_property32(
            PropMode::REPLACE,
            xid,
            self.net_wm_state,
            AtomEnum::ATOM,
            &states,
        );

        // 2. Official EWMH ClientMessage events sent to root window (for mapped windows)
        // Move to all desktops (0xFFFFFFFF)
        let ev_desktop = ClientMessageEvent {
            response_type: x11rb::protocol::xproto::CLIENT_MESSAGE_EVENT,
            format: 32,
            sequence: 0,
            window: xid,
            type_: self.net_wm_desktop,
            data: ClientMessageData::from([0xFFFFFFFF, 1, 0, 0, 0]),
        };
        let _ = self.conn.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev_desktop,
        );

        // Add STICKY state
        let ev_sticky = ClientMessageEvent {
            response_type: x11rb::protocol::xproto::CLIENT_MESSAGE_EVENT,
            format: 32,
            sequence: 0,
            window: xid,
            type_: self.net_wm_state,
            data: ClientMessageData::from([1, self.net_wm_state_sticky, 0, 1, 0]),
        };
        let _ = self.conn.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev_sticky,
        );

        // Add ABOVE state
        let ev_above = ClientMessageEvent {
            response_type: x11rb::protocol::xproto::CLIENT_MESSAGE_EVENT,
            format: 32,
            sequence: 0,
            window: xid,
            type_: self.net_wm_state,
            data: ClientMessageData::from([1, self.net_wm_state_above, 0, 1, 0]),
        };
        let _ = self.conn.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev_above,
        );

        // Add SKIP_TASKBAR and SKIP_PAGER states
        let ev_skip = ClientMessageEvent {
            response_type: x11rb::protocol::xproto::CLIENT_MESSAGE_EVENT,
            format: 32,
            sequence: 0,
            window: xid,
            type_: self.net_wm_state,
            data: ClientMessageData::from([
                1,
                self.net_wm_state_skip_taskbar,
                self.net_wm_state_skip_pager,
                1,
                0,
            ]),
        };
        let _ = self.conn.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev_skip,
        );

        let _ = self.conn.flush();
    }
}

#[cfg(target_os = "linux")]
pub fn apply_x11_sticky(window: &winit::window::Window) {
    if let Some(ctx) = X11Context::try_new() {
        ctx.apply_sticky(window);
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

    #[test]
    fn test_x11_context() {
        #[cfg(target_os = "linux")]
        if std::env::var_os("DISPLAY").is_some() {
            if let Some(ctx) = X11Context::try_new() {
                let pointer = ctx.query_pointer();
                assert!(pointer.is_some());
            }
        }
    }
}
