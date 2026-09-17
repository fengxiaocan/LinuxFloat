use fontdue::{Font, FontSettings};
use linux_monitor::config::{self, Config, Layout, Theme};
use linux_monitor::metrics::MonitorState;
use linux_monitor::monitor::Monitor;
use linux_monitor::platform::SessionInfo;
use linux_monitor::render;
use linux_monitor::startup;
use softbuffer::Surface;
use std::env;
use std::error::Error;
use std::num::NonZeroU32;
#[cfg(not(target_os = "windows"))]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowLevel};

const MENU_ITEM_COUNT: usize = 6;

enum AppEvent {
    #[cfg(target_os = "linux")]
    TrayMenu(tray_icon::menu::MenuEvent),
}

#[cfg(target_os = "linux")]
struct TrayState {
    _icon: tray_icon::TrayIcon,
    show: tray_icon::menu::MenuId,
    passthrough: tray_icon::menu::MenuId,
    locked: tray_icon::menu::MenuId,
    always_on_top: tray_icon::menu::MenuId,
    quit: tray_icon::menu::MenuId,
}

#[allow(deprecated)]
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    let (mut config, warning) = config::load();
    if let Some(warning) = warning {
        eprintln!("linux-monitor: {warning}");
    }

    let session = SessionInfo::detect();
    if args.iter().any(|arg| arg == "--version") {
        println!("linux-monitor {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--print-session") {
        println!(
            "backend={:?}\ndesktop={}\nlayer_shell={}\n{}",
            session.backend,
            if session.desktop.is_empty() {
                "unknown"
            } else {
                &session.desktop
            },
            session.supports_layer_shell,
            session.overlay_note()
        );
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--print-config") {
        println!("{}", toml::to_string_pretty(&config)?);
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--no-window") {
        let mut monitor = Monitor::new(config);
        println!("{}", one_line_state(&monitor.sample()));
        return Ok(());
    }

    config.validate();
    let receiver = spawn_monitor(config.clone());
    let event_loop = EventLoop::<AppEvent>::with_user_event().build()?;
    #[cfg(target_os = "linux")]
    let tray = match create_tray(&event_loop) {
        Ok(tray) => Some(tray),
        Err(error) => {
            eprintln!("linux-monitor: 创建系统托盘失败，将使用悬浮窗右键菜单: {error}");
            None
        }
    };
    let window_attributes = Window::default_attributes()
        .with_title("Linux Monitor")
        .with_decorations(false)
        .with_resizable(false)
        .with_transparent(true)
        .with_window_level(if config.always_on_top {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        })
        .with_inner_size(PhysicalSize::new(420_u32, 48_u32));
    let window = Arc::new(event_loop.create_window(window_attributes)?);

    place_window(&window, &config);
    apply_input_policy(&window, &config);
    if !config.start_visible {
        window.set_visible(false);
    }

    let context = softbuffer::Context::new(window.clone())?;
    let surface = Surface::new(&context, window.clone())?;
    let mut app = OverlayApp::new(
        window,
        surface,
        receiver,
        config,
        session,
        #[cfg(target_os = "linux")]
        tray,
    )?;
    app.resize_to_content();
    event_loop.run(move |event, event_loop| {
        event_loop.set_control_flow(ControlFlow::Wait);
        app.handle(event, event_loop);
    })?;
    Ok(())
}

fn spawn_monitor(config: Config) -> Receiver<MonitorState> {
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("monitor-sampler".to_string())
        .spawn(move || {
            let interval = Duration::from_millis(config.refresh_interval_ms);
            let mut monitor = Monitor::new(config);
            loop {
                let started = Instant::now();
                if sender.send(monitor.sample()).is_err() {
                    break;
                }
                let elapsed = started.elapsed();
                if elapsed < interval {
                    thread::sleep(interval - elapsed);
                }
            }
        })
        .expect("无法创建监控采样线程");
    receiver
}

#[cfg(target_os = "linux")]
fn create_tray(event_loop: &EventLoop<AppEvent>) -> Result<TrayState, Box<dyn Error>> {
    use tray_icon::menu::{Menu, MenuItem};

    let show = MenuItem::with_id("linux-monitor-show", "Show / Hide", true, None);
    let passthrough =
        MenuItem::with_id("linux-monitor-passthrough", "Mouse passthrough", true, None);
    let locked = MenuItem::with_id("linux-monitor-locked", "Lock position", true, None);
    let always_on_top = MenuItem::with_id("linux-monitor-top", "Always on top", true, None);
    let quit = MenuItem::with_id("linux-monitor-quit", "Quit", true, None);
    let menu = Menu::new();
    menu.append(&show)?;
    menu.append(&passthrough)?;
    menu.append(&locked)?;
    menu.append(&always_on_top)?;
    menu.append(&quit)?;

    let mut rgba = vec![0_u8; 16 * 16 * 4];
    for y in 0..16 {
        for x in 0..16 {
            if (3..13).contains(&x) && (3..13).contains(&y) {
                let index = (y * 16 + x) * 4;
                rgba[index] = 124;
                rgba[index + 1] = 223;
                rgba[index + 2] = 155;
                rgba[index + 3] = 255;
            }
        }
    }
    let icon = tray_icon::Icon::from_rgba(rgba, 16, 16)?;
    let proxy = event_loop.create_proxy();
    tray_icon::menu::MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(AppEvent::TrayMenu(event));
    }));
    let icon_handle = tray_icon::TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_icon(icon)
        .with_tooltip("Linux Monitor")
        .build()?;

    Ok(TrayState {
        _icon: icon_handle,
        show: show.id().clone(),
        passthrough: passthrough.id().clone(),
        locked: locked.id().clone(),
        always_on_top: always_on_top.id().clone(),
        quit: quit.id().clone(),
    })
}

fn place_window(window: &Window, config: &Config) {
    let window_size = window.inner_size();
    let monitors: Vec<_> = window.available_monitors().collect();
    if let (Some(x), Some(y)) = (config.position.x, config.position.y) {
        let visible = monitors.iter().any(|monitor| {
            let monitor_position = monitor.position();
            let monitor_size = monitor.size();
            rectangles_intersect(
                x,
                y,
                window_size.width,
                window_size.height,
                monitor_position.x,
                monitor_position.y,
                monitor_size.width,
                monitor_size.height,
            )
        });
        if visible || monitors.is_empty() {
            window.set_outer_position(PhysicalPosition::new(x, y));
            return;
        }
    }

    if let Some(monitor) = monitors.first() {
        let monitor_position = monitor.position();
        let monitor_size = monitor.size();
        let x =
            monitor_position.x + monitor_size.width.saturating_sub(window_size.width) as i32 - 16;
        let y = monitor_position.y + 16;
        window.set_outer_position(PhysicalPosition::new(x.max(monitor_position.x), y));
    }
}

#[allow(clippy::too_many_arguments)]
fn rectangles_intersect(
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    other_left: i32,
    other_top: i32,
    other_width: u32,
    other_height: u32,
) -> bool {
    let right = left.saturating_add(width as i32);
    let bottom = top.saturating_add(height as i32);
    let other_right = other_left.saturating_add(other_width as i32);
    let other_bottom = other_top.saturating_add(other_height as i32);
    left < other_right && right > other_left && top < other_bottom && bottom > other_top
}

fn one_line_state(state: &MonitorState) -> String {
    format!(
        "↓ {}  ↑ {}  │ CPU {}  │ RAM {}  │ GPU {}",
        linux_monitor::metrics::format_rate(state.download_bytes_per_second),
        linux_monitor::metrics::format_rate(state.upload_bytes_per_second),
        linux_monitor::metrics::format_percent(state.cpu_usage_percent),
        linux_monitor::metrics::format_percent(state.memory_usage_percent),
        state
            .gpu
            .as_ref()
            .map(|gpu| linux_monitor::metrics::format_percent(gpu.usage_percent))
            .unwrap_or_else(|| "N/A".to_string())
    )
}

struct OverlayApp {
    window: Arc<Window>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    receiver: Receiver<MonitorState>,
    config: Config,
    session: SessionInfo,
    state: MonitorState,
    font: Font,
    cursor_position: PhysicalPosition<f64>,
    dragging: bool,
    menu_open: bool,
    frame_size: PhysicalSize<u32>,
    dirty: bool,
    #[cfg(target_os = "linux")]
    tray: Option<TrayState>,
}

impl OverlayApp {
    fn new(
        window: Arc<Window>,
        surface: Surface<Arc<Window>, Arc<Window>>,
        receiver: Receiver<MonitorState>,
        config: Config,
        session: SessionInfo,
        #[cfg(target_os = "linux")] tray: Option<TrayState>,
    ) -> Result<Self, Box<dyn Error>> {
        let font = load_font()?;
        Ok(Self {
            window,
            surface,
            receiver,
            config,
            session,
            state: MonitorState::default(),
            font,
            cursor_position: PhysicalPosition::new(0.0, 0.0),
            dragging: false,
            menu_open: false,
            frame_size: PhysicalSize::new(0, 0),
            dirty: true,
            #[cfg(target_os = "linux")]
            tray,
        })
    }

    fn handle(&mut self, event: Event<AppEvent>, event_loop: &winit::event_loop::ActiveEventLoop) {
        match event {
            #[cfg(target_os = "linux")]
            Event::UserEvent(AppEvent::TrayMenu(event)) => self.handle_tray_menu(event, event_loop),
            Event::WindowEvent { event, .. } => self.handle_window_event(event, event_loop),
            Event::AboutToWait => {
                while let Ok(state) = self.receiver.try_recv() {
                    if state != self.state {
                        self.state = state;
                        self.dirty = true;
                    }
                }
                if self.dirty {
                    self.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    #[cfg(target_os = "linux")]
    fn handle_tray_menu(
        &mut self,
        event: tray_icon::menu::MenuEvent,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
        let Some(tray) = self.tray.as_ref() else {
            return;
        };
        if event.id == tray.show {
            let visible = self.window.is_visible().unwrap_or(true);
            self.window.set_visible(!visible);
        } else if event.id == tray.passthrough {
            self.config.mouse_passthrough = !self.config.mouse_passthrough;
            apply_input_policy(&self.window, &self.config);
            self.save_config();
        } else if event.id == tray.locked {
            self.config.locked = !self.config.locked;
            self.save_config();
        } else if event.id == tray.always_on_top {
            self.config.always_on_top = !self.config.always_on_top;
            apply_input_policy(&self.window, &self.config);
            self.save_config();
        } else if event.id == tray.quit {
            self.save_config();
            event_loop.exit();
        }
    }

    fn handle_window_event(
        &mut self,
        event: WindowEvent,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.resize_surface(size);
                self.dirty = true;
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_position = position;
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } if !self.config.mouse_passthrough => {
                self.menu_open = !self.menu_open;
                self.resize_to_content();
                self.dirty = true;
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => self.left_click(),
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } if self.dragging => {
                self.dragging = false;
                if let Ok(position) = self.window.outer_position() {
                    self.config.position.x = Some(position.x);
                    self.config.position.y = Some(position.y);
                    self.save_config();
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if let Some(text) = event.text.as_ref()
                    && (text == "m" || text == "M")
                {
                    self.config.mouse_passthrough = !self.config.mouse_passthrough;
                    self.save_config();
                    apply_input_policy(&self.window, &self.config);
                    self.dirty = true;
                }
            }
            _ => {}
        }
    }

    fn left_click(&mut self) {
        if self.menu_open {
            if let Some(index) = self.menu_index() {
                self.activate_menu(index);
            } else {
                self.menu_open = false;
                self.resize_to_content();
                self.dirty = true;
            }
            return;
        }

        if !self.config.locked && !self.config.mouse_passthrough {
            self.dragging = true;
            let _ = self.window.drag_window();
        }
    }

    fn menu_index(&self) -> Option<usize> {
        let line_height = self.line_height();
        let top = self.content_height() as f64 - line_height as f64 * MENU_ITEM_COUNT as f64;
        let y = self.cursor_position.y;
        if y < top {
            return None;
        }
        let index = ((y - top) / line_height as f64) as usize;
        (index < MENU_ITEM_COUNT).then_some(index)
    }

    fn activate_menu(&mut self, index: usize) {
        match index {
            0 => self.config.mouse_passthrough = !self.config.mouse_passthrough,
            1 => self.config.locked = !self.config.locked,
            2 => self.config.always_on_top = !self.config.always_on_top,
            3 => self.config.cycle_layout(),
            4 => self.config.cycle_theme(),
            5 => {
                self.save_config();
                std::process::exit(0);
            }
            _ => return,
        }
        self.save_config();
        apply_input_policy(&self.window, &self.config);
        self.menu_open = false;
        self.resize_to_content();
        self.dirty = true;
    }

    fn save_config(&self) {
        if let Err(error) = config::save(&self.config) {
            eprintln!("linux-monitor: 保存配置失败: {error}");
        }
        if let Err(error) = startup::sync_with_config(&self.config, env::current_exe().ok())
            && self.config.autostart
        {
            eprintln!("linux-monitor: 设置自动启动失败: {error}");
        }
    }

    fn redraw(&mut self) {
        if !self.dirty {
            return;
        }
        let frame = Frame::render(
            &self.font,
            &self.state,
            &self.config,
            &self.session,
            self.menu_open,
        );
        if self.frame_size != PhysicalSize::new(frame.width, frame.height) {
            self.frame_size = PhysicalSize::new(frame.width, frame.height);
            let _ = self.window.request_inner_size(self.frame_size);
            self.resize_surface(self.frame_size);
        }

        if self.resize_surface(self.frame_size)
            && let Ok(mut buffer) = self.surface.buffer_mut()
        {
            let count = buffer.len().min(frame.pixels.len());
            buffer[..count].copy_from_slice(&frame.pixels[..count]);
            if let Err(error) = buffer.present() {
                eprintln!("linux-monitor: 窗口刷新失败: {error}");
            }
        }
        self.dirty = false;
    }

    fn resize_surface(&mut self, size: PhysicalSize<u32>) -> bool {
        let width = NonZeroU32::new(size.width.max(1));
        let height = NonZeroU32::new(size.height.max(1));
        match (width, height) {
            (Some(width), Some(height)) => self.surface.resize(width, height).is_ok(),
            _ => false,
        }
    }

    fn resize_to_content(&mut self) {
        let lines = render::display_lines(&self.state, &self.config);
        let (width, height) = render::dimensions(
            &lines,
            self.config.font_size,
            self.config.layout,
            self.menu_open,
        );
        self.frame_size = PhysicalSize::new(width, height);
        let _ = self.window.request_inner_size(self.frame_size);
        self.resize_surface(self.frame_size);
    }

    fn line_height(&self) -> u32 {
        (self.config.font_size * 1.45).ceil() as u32
    }

    fn content_height(&self) -> u32 {
        let lines = render::display_lines(&self.state, &self.config);
        let (_, height) = render::dimensions(
            &lines,
            self.config.font_size,
            self.config.layout,
            self.menu_open,
        );
        height
    }
}

fn apply_input_policy(window: &Window, config: &Config) {
    if let Err(error) = window.set_cursor_hittest(!config.mouse_passthrough) {
        eprintln!("linux-monitor: 无法设置鼠标穿透: {error}");
    }
    window.set_window_level(if config.always_on_top {
        WindowLevel::AlwaysOnTop
    } else {
        WindowLevel::Normal
    });
}

fn load_font() -> Result<Font, Box<dyn Error>> {
    let candidates = font_candidates();
    for path in candidates {
        if let Ok(bytes) = std::fs::read(&path)
            && let Ok(font) = Font::from_bytes(bytes, FontSettings::default())
        {
            return Ok(font);
        }
    }
    Err("没有找到可用的系统字体".into())
}

fn font_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    #[cfg(target_os = "windows")]
    {
        candidates.extend([
            PathBuf::from(r"C:\Windows\Fonts\segoeui.ttf"),
            PathBuf::from(r"C:\Windows\Fonts\arial.ttf"),
        ]);
    }
    #[cfg(not(target_os = "windows"))]
    {
        candidates.extend([
            PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
            PathBuf::from("/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf"),
            PathBuf::from("/usr/share/fonts/opentype/noto/NotoSans-Regular.ttf"),
        ]);
        let mut roots = vec![
            PathBuf::from("/usr/share/fonts"),
            PathBuf::from("/usr/local/share/fonts"),
        ];
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join(".local/share/fonts"));
        }
        for root in roots {
            collect_font_files(&root, 0, &mut candidates);
        }
    }
    candidates.dedup();
    candidates
}

#[cfg(not(target_os = "windows"))]
fn collect_font_files(directory: &Path, depth: u8, candidates: &mut Vec<PathBuf>) {
    if depth > 3 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_font_files(&path, depth + 1, candidates);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ttf"))
        {
            candidates.push(path);
        }
    }
}

struct Frame {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
}

impl Frame {
    fn render(
        font: &Font,
        state: &MonitorState,
        config: &Config,
        session: &SessionInfo,
        menu_open: bool,
    ) -> Self {
        let lines = render::display_lines(state, config);
        let (width, height) =
            render::dimensions(&lines, config.font_size, config.layout, menu_open);
        let mut frame = Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize],
        };
        let colors = render::theme_colors(config, is_system_dark(session));
        frame.fill_rounded_rect(0, 0, width, height, config.corner_radius, colors.background);

        let padding_x = (config.font_size * 1.15).ceil() as i32;
        let line_height = (config.font_size * 1.45).ceil() as i32;
        let mut y = (config.font_size * 0.45).ceil() as i32;
        for line in &lines {
            let color = render::metric_color(line.kind, state, config, colors);
            frame.draw_text(font, &line.text, padding_x, y, config.font_size, color);
            y += line_height;
        }

        if menu_open {
            let menu_y = height as i32 - line_height * MENU_ITEM_COUNT as i32;
            frame.fill_rect(
                padding_x / 2,
                menu_y,
                width as i32 - padding_x,
                height as i32,
                colors.separator,
            );
            let menu = [
                format!("Passthrough: {}", on_off(config.mouse_passthrough)),
                format!("Locked: {}", on_off(config.locked)),
                format!("Always on top: {}", on_off(config.always_on_top)),
                format!("Layout: {}", layout_name(config.layout)),
                format!("Theme: {}", theme_name(config.theme)),
                "Quit".to_string(),
            ];
            for (index, item) in menu.iter().enumerate() {
                frame.draw_text(
                    font,
                    item,
                    padding_x,
                    menu_y
                        + line_height * index as i32
                        + (line_height - config.font_size as i32) / 2,
                    config.font_size,
                    colors.foreground,
                );
            }
        }
        frame
    }

    fn fill_rect(&mut self, left: i32, top: i32, right: i32, bottom: i32, color: u32) {
        for y in top.max(0)..bottom.min(self.height as i32) {
            for x in left.max(0)..right.min(self.width as i32) {
                self.blend_pixel(x, y, color, 255);
            }
        }
    }

    fn fill_rounded_rect(
        &mut self,
        left: i32,
        top: i32,
        width: u32,
        height: u32,
        radius: u32,
        color: u32,
    ) {
        let radius = radius.min(width / 2).min(height / 2) as i32;
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let dx = if x < radius {
                    radius - x
                } else if x >= width as i32 - radius {
                    x - (width as i32 - radius - 1)
                } else {
                    0
                };
                let dy = if y < radius {
                    radius - y
                } else if y >= height as i32 - radius {
                    y - (height as i32 - radius - 1)
                } else {
                    0
                };
                if dx == 0 || dy == 0 || dx * dx + dy * dy <= radius * radius {
                    self.blend_pixel(left + x, top + y, color, 255);
                }
            }
        }
    }

    fn draw_text(&mut self, font: &Font, text: &str, x: i32, y: i32, size: f32, color: u32) {
        let mut cursor = x;
        for character in text.chars() {
            let (metrics, bitmap) = font.rasterize(character, size);
            let glyph_top = y + ((size * 1.45 - metrics.height as f32) / 2.0).round() as i32;
            for glyph_y in 0..metrics.height {
                for glyph_x in 0..metrics.width {
                    let alpha = bitmap[glyph_y * metrics.width + glyph_x];
                    if alpha > 0 {
                        self.blend_pixel(
                            cursor + metrics.xmin + glyph_x as i32,
                            glyph_top + glyph_y as i32,
                            color,
                            alpha,
                        );
                    }
                }
            }
            cursor += metrics.advance_width.ceil() as i32;
        }
    }

    fn blend_pixel(&mut self, x: i32, y: i32, color: u32, alpha: u8) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let index = y as usize * self.width as usize + x as usize;
        let encoded_alpha = color >> 24;
        let color_alpha = if encoded_alpha == 0 && (color & 0x00_FF_FF_FF) != 0 {
            255
        } else {
            encoded_alpha
        };
        let source_alpha = (color_alpha * alpha as u32) / 255;
        if source_alpha == 255 {
            self.pixels[index] = color & 0x00_FF_FF_FF;
            return;
        }
        if source_alpha == 0 {
            return;
        }
        let source = color & 0x00_FF_FF_FF;
        let destination = self.pixels[index] & 0x00_FF_FF_FF;
        let blend = |source_channel: u32, destination_channel: u32| {
            (source_channel * source_alpha + destination_channel * (255 - source_alpha)) / 255
        };
        let red = blend((source >> 16) & 0xFF, (destination >> 16) & 0xFF);
        let green = blend((source >> 8) & 0xFF, (destination >> 8) & 0xFF);
        let blue = blend(source & 0xFF, destination & 0xFF);
        self.pixels[index] = (red << 16) | (green << 8) | blue;
    }
}

fn is_system_dark(_session: &SessionInfo) -> bool {
    env::var("GTK_THEME")
        .or_else(|_| env::var("COLOR_SCHEME"))
        .map(|value| value.to_ascii_lowercase().contains("dark"))
        .unwrap_or(true)
}

fn on_off(value: bool) -> &'static str {
    if value { "ON" } else { "OFF" }
}

fn layout_name(layout: Layout) -> &'static str {
    match layout {
        Layout::Horizontal => "Horizontal",
        Layout::Vertical => "Vertical",
    }
}

fn theme_name(theme: Theme) -> &'static str {
    match theme {
        Theme::Dark => "Dark",
        Theme::Light => "Light",
        Theme::System => "System",
        Theme::Transparent => "Transparent",
    }
}
