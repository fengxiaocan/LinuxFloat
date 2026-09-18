use fontdue::{Font, FontSettings};
use linux_monitor::config::{self, Config, Layout, Theme, TopAlignment, WindowStyle};
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
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::monitor::MonitorHandle;
use winit::window::{Window, WindowLevel};

const MENU_ITEM_COUNT: usize = 9;

enum AppEvent {
    #[cfg(target_os = "linux")]
    TrayMenu(tray_icon::menu::MenuEvent),
}

#[cfg(target_os = "linux")]
struct TrayState {
    _icon: tray_icon::TrayIcon,
    show_item: tray_icon::menu::MenuItem,
    always_on_top_item: tray_icon::menu::CheckMenuItem,
    all_screens_top_item: tray_icon::menu::CheckMenuItem,
    passthrough_item: tray_icon::menu::CheckMenuItem,
    locked_item: tray_icon::menu::CheckMenuItem,
    autostart_item: tray_icon::menu::CheckMenuItem,

    layout_horiz: tray_icon::menu::CheckMenuItem,
    layout_vert: tray_icon::menu::CheckMenuItem,

    theme_dark: tray_icon::menu::CheckMenuItem,
    theme_catppuccin: tray_icon::menu::CheckMenuItem,
    theme_cyberpunk: tray_icon::menu::CheckMenuItem,
    theme_nord: tray_icon::menu::CheckMenuItem,
    theme_matrix: tray_icon::menu::CheckMenuItem,
    theme_solarized: tray_icon::menu::CheckMenuItem,
    theme_light: tray_icon::menu::CheckMenuItem,
    theme_trans: tray_icon::menu::CheckMenuItem,
    theme_sys: tray_icon::menu::CheckMenuItem,

    style_card: tray_icon::menu::CheckMenuItem,
    style_capsule: tray_icon::menu::CheckMenuItem,
    style_minibar: tray_icon::menu::CheckMenuItem,
    style_flat: tray_icon::menu::CheckMenuItem,

    opacity_100: tray_icon::menu::CheckMenuItem,
    opacity_90: tray_icon::menu::CheckMenuItem,
    opacity_80: tray_icon::menu::CheckMenuItem,
    opacity_70: tray_icon::menu::CheckMenuItem,
    opacity_60: tray_icon::menu::CheckMenuItem,
    opacity_50: tray_icon::menu::CheckMenuItem,
    opacity_35: tray_icon::menu::CheckMenuItem,
    opacity_20: tray_icon::menu::CheckMenuItem,

    align_center: tray_icon::menu::CheckMenuItem,
    align_right: tray_icon::menu::CheckMenuItem,
    align_left: tray_icon::menu::CheckMenuItem,

    refresh_500: tray_icon::menu::CheckMenuItem,
    refresh_1000: tray_icon::menu::CheckMenuItem,
    refresh_2000: tray_icon::menu::CheckMenuItem,

    quit_id: tray_icon::menu::MenuId,
}

#[cfg(target_os = "linux")]
impl TrayState {
    fn sync(&self, config: &Config, is_visible: bool) {
        self.show_item
            .set_text(if is_visible { "隐藏悬浮窗" } else { "显示悬浮窗" });
        self.always_on_top_item.set_checked(config.always_on_top);
        self.all_screens_top_item
            .set_checked(config.all_screens_top);
        self.passthrough_item
            .set_checked(config.mouse_passthrough);
        self.locked_item.set_checked(config.locked);
        self.autostart_item.set_checked(config.autostart);

        self.layout_horiz
            .set_checked(config.layout == Layout::Horizontal);
        self.layout_vert
            .set_checked(config.layout == Layout::Vertical);

        self.theme_dark.set_checked(config.theme == Theme::Dark);
        self.theme_catppuccin
            .set_checked(config.theme == Theme::Catppuccin);
        self.theme_cyberpunk
            .set_checked(config.theme == Theme::Cyberpunk);
        self.theme_nord.set_checked(config.theme == Theme::Nord);
        self.theme_matrix.set_checked(config.theme == Theme::Matrix);
        self.theme_solarized
            .set_checked(config.theme == Theme::Solarized);
        self.theme_light.set_checked(config.theme == Theme::Light);
        self.theme_trans
            .set_checked(config.theme == Theme::Transparent);
        self.theme_sys.set_checked(config.theme == Theme::System);

        self.style_card
            .set_checked(config.window_style == WindowStyle::Card);
        self.style_capsule
            .set_checked(config.window_style == WindowStyle::Capsule);
        self.style_minibar
            .set_checked(config.window_style == WindowStyle::MiniBar);
        self.style_flat
            .set_checked(config.window_style == WindowStyle::Flat);

        self.opacity_100
            .set_checked((config.opacity - 1.0).abs() < 0.05);
        self.opacity_90
            .set_checked((config.opacity - 0.90).abs() < 0.05);
        self.opacity_80
            .set_checked((config.opacity - 0.80).abs() < 0.05);
        self.opacity_70
            .set_checked((config.opacity - 0.70).abs() < 0.05);
        self.opacity_60
            .set_checked((config.opacity - 0.60).abs() < 0.05);
        self.opacity_50
            .set_checked((config.opacity - 0.50).abs() < 0.05);
        self.opacity_35
            .set_checked((config.opacity - 0.35).abs() < 0.07);
        self.opacity_20
            .set_checked((config.opacity - 0.20).abs() < 0.07);

        self.align_center
            .set_checked(config.top_alignment == TopAlignment::Center);
        self.align_right
            .set_checked(config.top_alignment == TopAlignment::Right);
        self.align_left
            .set_checked(config.top_alignment == TopAlignment::Left);

        self.refresh_500
            .set_checked(config.refresh_interval_ms == 500);
        self.refresh_1000
            .set_checked(config.refresh_interval_ms == 1000);
        self.refresh_2000
            .set_checked(config.refresh_interval_ms == 2000);
    }
}

#[allow(deprecated)]
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    let (mut config, warning) = config::load();
    if let Some(warning) = warning {
        eprintln!("linux-monitor: {warning}");
    }

    let session = SessionInfo::detect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("linux-monitor {}", env!("CARGO_PKG_VERSION"));
        println!("A lightweight Linux desktop performance monitor overlay\n");
        println!("USAGE:");
        println!("    linux-monitor [OPTIONS]\n");
        println!("OPTIONS:");
        println!("    -h, --help           Print help information");
        println!("    -V, --version        Print version information");
        println!("    --print-session      Print detected session info and exit");
        println!("    --print-config       Print loaded configuration and exit");
        println!("    --no-window          Sample and print current metrics once, then exit");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
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
    let tray = match create_tray(&event_loop, &config) {
        Ok(tray) => Some(tray),
        Err(error) => {
            eprintln!("linux-monitor: 创建系统托盘失败，将使用悬浮窗右键菜单: {error}");
            None
        }
    };

    let primary_window = Arc::new(event_loop.create_window(build_window_attributes(&config))?);
    setup_window(&primary_window, &config);
    let monitors: Vec<MonitorHandle> = primary_window.available_monitors().collect();
    let first_mon = monitors.first();
    place_window(&primary_window, &config, first_mon);
    if !config.start_visible {
        primary_window.set_visible(false);
    }

    let context = softbuffer::Context::new(primary_window.clone())?;
    let surface = Surface::new(&context, primary_window.clone())?;

    let initial_pos = primary_window
        .outer_position()
        .unwrap_or(PhysicalPosition::new(0, 0));
    let mut instances = vec![WindowInstance {
        window: primary_window.clone(),
        surface,
        monitor_name: first_mon.and_then(|m: &MonitorHandle| m.name()),
        cursor_position: PhysicalPosition::new(0.0, 0.0),
        dragging: false,
        drag_start_root: (0, 0),
        drag_start_win_pos: initial_pos,
        drag_last_pos: initial_pos,
        menu_open: false,
        frame_size: PhysicalSize::new(0, 0),
        sticky_applied: false,
    }];

    if config.all_screens_top && monitors.len() > 1 {
        for mon in monitors.iter().skip(1) {
            if let Ok(win) = event_loop.create_window(build_window_attributes(&config)) {
                let win = Arc::new(win);
                setup_window(&win, &config);
                place_window(&win, &config, Some(mon));
                if !config.start_visible {
                    win.set_visible(false);
                }
                let mon_win_pos = win.outer_position().unwrap_or(PhysicalPosition::new(0, 0));
                if let Ok(surf) = Surface::new(&context, win.clone()) {
                    instances.push(WindowInstance {
                        window: win,
                        surface: surf,
                        monitor_name: mon.name(),
                        cursor_position: PhysicalPosition::new(0.0, 0.0),
                        dragging: false,
                        drag_start_root: (0, 0),
                        drag_start_win_pos: mon_win_pos,
                        drag_last_pos: mon_win_pos,
                        menu_open: false,
                        frame_size: PhysicalSize::new(0, 0),
                        sticky_applied: false,
                    });
                }
            }
        }
    }

    let mut app = OverlayApp::new(
        instances,
        context,
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
fn create_tray(
    event_loop: &EventLoop<AppEvent>,
    config: &Config,
) -> Result<TrayState, Box<dyn Error>> {
    use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

    let show_item = MenuItem::with_id(
        "tray-show",
        if config.start_visible {
            "隐藏悬浮窗"
        } else {
            "显示悬浮窗"
        },
        true,
        None,
    );
    let always_on_top_item =
        CheckMenuItem::with_id("tray-top", "始终置顶", true, config.always_on_top, None);
    let all_screens_top_item = CheckMenuItem::with_id(
        "tray-all-screens-top",
        "一直显示在所有屏幕顶部",
        true,
        config.all_screens_top,
        None,
    );
    let passthrough_item = CheckMenuItem::with_id(
        "tray-passthrough",
        "鼠标穿透",
        true,
        config.mouse_passthrough,
        None,
    );
    let locked_item =
        CheckMenuItem::with_id("tray-locked", "锁定位置", true, config.locked, None);

    // 布局子菜单
    let layout_menu = Submenu::new("显示布局", true);
    let layout_horiz = CheckMenuItem::with_id(
        "tray-layout-h",
        "横向布局",
        true,
        config.layout == Layout::Horizontal,
        None,
    );
    let layout_vert = CheckMenuItem::with_id(
        "tray-layout-v",
        "纵向布局",
        true,
        config.layout == Layout::Vertical,
        None,
    );
    layout_menu.append(&layout_horiz)?;
    layout_menu.append(&layout_vert)?;

    // 主题子菜单
    let theme_menu = Submenu::new("主题风格", true);
    let theme_dark = CheckMenuItem::with_id(
        "tray-theme-dark",
        "深色主题 (Dark)",
        true,
        config.theme == Theme::Dark,
        None,
    );
    let theme_catppuccin = CheckMenuItem::with_id(
        "tray-theme-catppuccin",
        "猫咖柔和 (Catppuccin)",
        true,
        config.theme == Theme::Catppuccin,
        None,
    );
    let theme_cyberpunk = CheckMenuItem::with_id(
        "tray-theme-cyberpunk",
        "赛博朋克 (Cyberpunk)",
        true,
        config.theme == Theme::Cyberpunk,
        None,
    );
    let theme_nord = CheckMenuItem::with_id(
        "tray-theme-nord",
        "北欧极光 (Nord)",
        true,
        config.theme == Theme::Nord,
        None,
    );
    let theme_matrix = CheckMenuItem::with_id(
        "tray-theme-matrix",
        "黑客极客 (Matrix)",
        true,
        config.theme == Theme::Matrix,
        None,
    );
    let theme_solarized = CheckMenuItem::with_id(
        "tray-theme-solarized",
        "经典日光 (Solarized)",
        true,
        config.theme == Theme::Solarized,
        None,
    );
    let theme_light = CheckMenuItem::with_id(
        "tray-theme-light",
        "明亮浅色 (Light)",
        true,
        config.theme == Theme::Light,
        None,
    );
    let theme_trans = CheckMenuItem::with_id(
        "tray-theme-trans",
        "纯粹透明 (Transparent)",
        true,
        config.theme == Theme::Transparent,
        None,
    );
    let theme_sys = CheckMenuItem::with_id(
        "tray-theme-sys",
        "跟随系统 (System)",
        true,
        config.theme == Theme::System,
        None,
    );
    theme_menu.append(&theme_dark)?;
    theme_menu.append(&theme_catppuccin)?;
    theme_menu.append(&theme_cyberpunk)?;
    theme_menu.append(&theme_nord)?;
    theme_menu.append(&theme_matrix)?;
    theme_menu.append(&theme_solarized)?;
    theme_menu.append(&theme_light)?;
    theme_menu.append(&theme_trans)?;
    theme_menu.append(&theme_sys)?;

    // 窗口样式子菜单
    let style_menu = Submenu::new("窗口样式", true);
    let style_card = CheckMenuItem::with_id(
        "tray-style-card",
        "现代卡片 (Card)",
        true,
        config.window_style == WindowStyle::Card,
        None,
    );
    let style_capsule = CheckMenuItem::with_id(
        "tray-style-capsule",
        "胶囊微型 (Capsule)",
        true,
        config.window_style == WindowStyle::Capsule,
        None,
    );
    let style_minibar = CheckMenuItem::with_id(
        "tray-style-minibar",
        "动态进度条 (MiniBar)",
        true,
        config.window_style == WindowStyle::MiniBar,
        None,
    );
    let style_flat = CheckMenuItem::with_id(
        "tray-style-flat",
        "极简无界 (Flat)",
        true,
        config.window_style == WindowStyle::Flat,
        None,
    );
    style_menu.append(&style_card)?;
    style_menu.append(&style_capsule)?;
    style_menu.append(&style_minibar)?;
    style_menu.append(&style_flat)?;

    // 透明度子菜单
    let opacity_menu = Submenu::new("窗口透明度", true);
    let opacity_100 = CheckMenuItem::with_id(
        "tray-op-100",
        "100% (完全不透明)",
        true,
        (config.opacity - 1.0).abs() < 0.05,
        None,
    );
    let opacity_90 = CheckMenuItem::with_id(
        "tray-op-90",
        "90%",
        true,
        (config.opacity - 0.90).abs() < 0.05,
        None,
    );
    let opacity_80 = CheckMenuItem::with_id(
        "tray-op-80",
        "80%",
        true,
        (config.opacity - 0.80).abs() < 0.05,
        None,
    );
    let opacity_70 = CheckMenuItem::with_id(
        "tray-op-70",
        "70%",
        true,
        (config.opacity - 0.70).abs() < 0.05,
        None,
    );
    let opacity_60 = CheckMenuItem::with_id(
        "tray-op-60",
        "60%",
        true,
        (config.opacity - 0.60).abs() < 0.05,
        None,
    );
    let opacity_50 = CheckMenuItem::with_id(
        "tray-op-50",
        "50% (半透明)",
        true,
        (config.opacity - 0.50).abs() < 0.05,
        None,
    );
    let opacity_35 = CheckMenuItem::with_id(
        "tray-op-35",
        "35%",
        true,
        (config.opacity - 0.35).abs() < 0.07,
        None,
    );
    let opacity_20 = CheckMenuItem::with_id(
        "tray-op-20",
        "20% (高透)",
        true,
        (config.opacity - 0.20).abs() < 0.07,
        None,
    );
    opacity_menu.append(&opacity_100)?;
    opacity_menu.append(&opacity_90)?;
    opacity_menu.append(&opacity_80)?;
    opacity_menu.append(&opacity_70)?;
    opacity_menu.append(&opacity_60)?;
    opacity_menu.append(&opacity_50)?;
    opacity_menu.append(&opacity_35)?;
    opacity_menu.append(&opacity_20)?;

    // 屏幕对齐子菜单
    let align_menu = Submenu::new("屏幕对齐", true);
    let align_center = CheckMenuItem::with_id(
        "tray-align-center",
        "屏幕顶部居中",
        true,
        config.top_alignment == TopAlignment::Center,
        None,
    );
    let align_right = CheckMenuItem::with_id(
        "tray-align-right",
        "屏幕顶部靠右",
        true,
        config.top_alignment == TopAlignment::Right,
        None,
    );
    let align_left = CheckMenuItem::with_id(
        "tray-align-left",
        "屏幕顶部靠左",
        true,
        config.top_alignment == TopAlignment::Left,
        None,
    );
    align_menu.append(&align_center)?;
    align_menu.append(&align_right)?;
    align_menu.append(&align_left)?;

    // 刷新频率子菜单
    let refresh_menu = Submenu::new("刷新频率", true);
    let refresh_500 = CheckMenuItem::with_id(
        "tray-refresh-500",
        "0.5 秒 (500ms)",
        true,
        config.refresh_interval_ms == 500,
        None,
    );
    let refresh_1000 = CheckMenuItem::with_id(
        "tray-refresh-1000",
        "1.0 秒 (1000ms)",
        true,
        config.refresh_interval_ms == 1000,
        None,
    );
    let refresh_2000 = CheckMenuItem::with_id(
        "tray-refresh-2000",
        "2.0 秒 (2000ms)",
        true,
        config.refresh_interval_ms == 2000,
        None,
    );
    refresh_menu.append(&refresh_500)?;
    refresh_menu.append(&refresh_1000)?;
    refresh_menu.append(&refresh_2000)?;

    // 开机自启动
    let autostart_item =
        CheckMenuItem::with_id("tray-autostart", "开机自启", true, config.autostart, None);

    // 退出程序
    let quit_item = MenuItem::with_id("tray-quit", "退出程序", true, None);
    let quit_id = quit_item.id().clone();

    let menu = Menu::new();
    menu.append(&show_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&always_on_top_item)?;
    menu.append(&all_screens_top_item)?;
    menu.append(&passthrough_item)?;
    menu.append(&locked_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&layout_menu)?;
    menu.append(&theme_menu)?;
    menu.append(&style_menu)?;
    menu.append(&opacity_menu)?;
    menu.append(&align_menu)?;
    menu.append(&refresh_menu)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&autostart_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit_item)?;

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
        .with_tooltip("Linux Monitor 性能悬浮窗")
        .build()?;

    Ok(TrayState {
        _icon: icon_handle,
        show_item,
        always_on_top_item,
        all_screens_top_item,
        passthrough_item,
        locked_item,
        autostart_item,
        layout_horiz,
        layout_vert,
        theme_dark,
        theme_catppuccin,
        theme_cyberpunk,
        theme_nord,
        theme_matrix,
        theme_solarized,
        theme_light,
        theme_trans,
        theme_sys,
        style_card,
        style_capsule,
        style_minibar,
        style_flat,
        opacity_100,
        opacity_90,
        opacity_80,
        opacity_70,
        opacity_60,
        opacity_50,
        opacity_35,
        opacity_20,
        align_center,
        align_right,
        align_left,
        refresh_500,
        refresh_1000,
        refresh_2000,
        quit_id,
    })
}

fn build_window_attributes(config: &Config) -> winit::window::WindowAttributes {
    let window_attributes = Window::default_attributes()
        .with_title("Linux Monitor")
        .with_decorations(false)
        .with_resizable(false)
        .with_transparent(true)
        .with_window_level(if config.always_on_top || config.all_screens_top {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        })
        .with_inner_size(PhysicalSize::new(420_u32, 48_u32));

    #[cfg(target_os = "linux")]
    {
        use winit::platform::wayland::WindowAttributesExtWayland;
        use winit::platform::x11::{WindowAttributesExtX11, WindowType};
        let window_attributes = WindowAttributesExtX11::with_name(
            window_attributes,
            "linux-monitor",
            "linux-monitor",
        );
        let window_attributes = WindowAttributesExtX11::with_x11_window_type(
            window_attributes,
            vec![
                WindowType::Utility,
                WindowType::Normal,
            ],
        );
        WindowAttributesExtWayland::with_name(window_attributes, "linux-monitor", "linux-monitor")
    }
    #[cfg(not(target_os = "linux"))]
    window_attributes
}

fn setup_window(window: &Window, config: &Config) {
    if let Err(error) = window.set_cursor_hittest(!config.mouse_passthrough) {
        eprintln!("linux-monitor: 无法设置鼠标穿透: {error}");
    }
    #[cfg(target_os = "linux")]
    linux_monitor::platform::apply_x11_sticky(window);
}

fn place_window(window: &Window, config: &Config, monitor: Option<&MonitorHandle>) {
    let window_size = window.inner_size();
    let monitors: Vec<_> = window.available_monitors().collect();
    let target_monitor = monitor
        .cloned()
        .or_else(|| window.primary_monitor())
        .or_else(|| monitors.first().cloned());

    let Some(mon) = target_monitor else {
        return;
    };

    let mon_pos = mon.position();
    let mon_size = mon.size();

    if config.all_screens_top {
        let y = mon_pos.y + 8;
        let x = match config.top_alignment {
            TopAlignment::Center => {
                mon_pos.x + (mon_size.width.saturating_sub(window_size.width) / 2) as i32
            }
            TopAlignment::Right => {
                mon_pos.x + mon_size.width.saturating_sub(window_size.width) as i32 - 16
            }
            TopAlignment::Left => mon_pos.x + 16,
        };
        window.set_outer_position(PhysicalPosition::new(x, y));
        return;
    }

    if let (Some(x), Some(y)) = (config.position.x, config.position.y) {
        let visible = monitors.iter().any(|m| {
            let p = m.position();
            let s = m.size();
            rectangles_intersect(
                x,
                y,
                window_size.width,
                window_size.height,
                p.x,
                p.y,
                s.width,
                s.height,
            )
        });
        if visible || monitors.is_empty() {
            window.set_outer_position(PhysicalPosition::new(x, y));
            return;
        }
    }

    let x = mon_pos.x + mon_size.width.saturating_sub(window_size.width) as i32 - 16;
    let y = mon_pos.y + 16;
    window.set_outer_position(PhysicalPosition::new(x.max(mon_pos.x), y));
}

fn resize_surface(
    surface: &mut Surface<Arc<Window>, Arc<Window>>,
    size: PhysicalSize<u32>,
) -> bool {
    let width = NonZeroU32::new(size.width.max(1));
    let height = NonZeroU32::new(size.height.max(1));
    match (width, height) {
        (Some(width), Some(height)) => surface.resize(width, height).is_ok(),
        _ => false,
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

struct WindowInstance {
    window: Arc<Window>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    monitor_name: Option<String>,
    cursor_position: PhysicalPosition<f64>,
    dragging: bool,
    drag_start_root: (i32, i32),
    drag_start_win_pos: PhysicalPosition<i32>,
    drag_last_pos: PhysicalPosition<i32>,
    menu_open: bool,
    frame_size: PhysicalSize<u32>,
    sticky_applied: bool,
}

struct OverlayApp {
    windows: Vec<WindowInstance>,
    context: softbuffer::Context<Arc<Window>>,
    receiver: Receiver<MonitorState>,
    config: Config,
    session: SessionInfo,
    state: MonitorState,
    font: Font,
    dirty: bool,
    #[cfg(target_os = "linux")]
    tray: Option<TrayState>,
    #[cfg(target_os = "linux")]
    x11: Option<linux_monitor::platform::X11Context>,
}

impl OverlayApp {
    fn new(
        windows: Vec<WindowInstance>,
        context: softbuffer::Context<Arc<Window>>,
        receiver: Receiver<MonitorState>,
        config: Config,
        session: SessionInfo,
        #[cfg(target_os = "linux")] tray: Option<TrayState>,
    ) -> Result<Self, Box<dyn Error>> {
        let font = load_font()?;
        #[cfg(target_os = "linux")]
        let x11 = linux_monitor::platform::X11Context::try_new();
        Ok(Self {
            windows,
            context,
            receiver,
            config,
            session,
            state: MonitorState::default(),
            font,
            dirty: true,
            #[cfg(target_os = "linux")]
            tray,
            #[cfg(target_os = "linux")]
            x11,
        })
    }

    fn handle(&mut self, event: Event<AppEvent>, event_loop: &ActiveEventLoop) {
        match event {
            #[cfg(target_os = "linux")]
            Event::UserEvent(AppEvent::TrayMenu(event)) => {
                self.handle_tray_menu(event, event_loop);
            }
            Event::WindowEvent { window_id, event } => {
                if let Some(idx) = self.windows.iter().position(|w| w.window.id() == window_id) {
                    self.handle_window_event(idx, event, event_loop);
                }
            }
            Event::AboutToWait => {
                while let Ok(state) = self.receiver.try_recv() {
                    if state != self.state {
                        self.state = state;
                        self.dirty = true;
                    }
                }
                if self.dirty {
                    for inst in &self.windows {
                        inst.window.request_redraw();
                    }
                    self.dirty = false;
                }
            }
            _ => {}
        }
    }

    #[cfg(target_os = "linux")]
    fn handle_tray_menu(&mut self, event: tray_icon::menu::MenuEvent, event_loop: &ActiveEventLoop) {
        let Some(tray) = self.tray.as_ref() else {
            return;
        };
        let event_id = event.id;
        if event_id == *tray.show_item.id() {
            let is_visible = self
                .windows
                .first()
                .and_then(|w| w.window.is_visible())
                .unwrap_or(true);
            let new_visible = !is_visible;
            for w in &self.windows {
                w.window.set_visible(new_visible);
            }
        } else if event_id == *tray.always_on_top_item.id() {
            self.config.always_on_top = !self.config.always_on_top;
            self.apply_level_and_sticky();
        } else if event_id == *tray.all_screens_top_item.id() {
            self.config.all_screens_top = !self.config.all_screens_top;
            if self.config.all_screens_top {
                self.config.always_on_top = true;
            }
            self.reconfigure_windows(event_loop);
        } else if event_id == *tray.passthrough_item.id() {
            self.config.mouse_passthrough = !self.config.mouse_passthrough;
            for w in &self.windows {
                let _ = w.window.set_cursor_hittest(!self.config.mouse_passthrough);
            }
        } else if event_id == *tray.locked_item.id() {
            self.config.locked = !self.config.locked;
        } else if event_id == *tray.autostart_item.id() {
            self.config.autostart = !self.config.autostart;
            let _ = startup::set_autostart(self.config.autostart, None);
        } else if event_id == *tray.layout_horiz.id() {
            self.config.layout = Layout::Horizontal;
            self.resize_to_content();
        } else if event_id == *tray.layout_vert.id() {
            self.config.layout = Layout::Vertical;
            self.resize_to_content();
        } else if event_id == *tray.theme_dark.id() {
            self.config.theme = Theme::Dark;
        } else if event_id == *tray.theme_catppuccin.id() {
            self.config.theme = Theme::Catppuccin;
        } else if event_id == *tray.theme_cyberpunk.id() {
            self.config.theme = Theme::Cyberpunk;
        } else if event_id == *tray.theme_nord.id() {
            self.config.theme = Theme::Nord;
        } else if event_id == *tray.theme_matrix.id() {
            self.config.theme = Theme::Matrix;
        } else if event_id == *tray.theme_solarized.id() {
            self.config.theme = Theme::Solarized;
        } else if event_id == *tray.theme_light.id() {
            self.config.theme = Theme::Light;
        } else if event_id == *tray.theme_trans.id() {
            self.config.theme = Theme::Transparent;
        } else if event_id == *tray.theme_sys.id() {
            self.config.theme = Theme::System;
        } else if event_id == *tray.style_card.id() {
            self.config.window_style = WindowStyle::Card;
            self.config.show_mini_bars = false;
            self.resize_to_content();
        } else if event_id == *tray.style_capsule.id() {
            self.config.window_style = WindowStyle::Capsule;
            self.config.show_mini_bars = false;
            self.resize_to_content();
        } else if event_id == *tray.style_minibar.id() {
            self.config.window_style = WindowStyle::MiniBar;
            self.config.show_mini_bars = true;
            self.resize_to_content();
        } else if event_id == *tray.style_flat.id() {
            self.config.window_style = WindowStyle::Flat;
            self.config.show_mini_bars = false;
            self.resize_to_content();
        } else if event_id == *tray.opacity_100.id() {
            self.config.opacity = 1.0;
        } else if event_id == *tray.opacity_90.id() {
            self.config.opacity = 0.90;
        } else if event_id == *tray.opacity_80.id() {
            self.config.opacity = 0.80;
        } else if event_id == *tray.opacity_70.id() {
            self.config.opacity = 0.70;
        } else if event_id == *tray.opacity_60.id() {
            self.config.opacity = 0.60;
        } else if event_id == *tray.opacity_50.id() {
            self.config.opacity = 0.50;
        } else if event_id == *tray.opacity_35.id() {
            self.config.opacity = 0.35;
        } else if event_id == *tray.opacity_20.id() {
            self.config.opacity = 0.20;
        } else if event_id == *tray.align_center.id() {
            self.config.top_alignment = TopAlignment::Center;
            self.reposition_windows();
        } else if event_id == *tray.align_right.id() {
            self.config.top_alignment = TopAlignment::Right;
            self.reposition_windows();
        } else if event_id == *tray.align_left.id() {
            self.config.top_alignment = TopAlignment::Left;
            self.reposition_windows();
        } else if event_id == *tray.refresh_500.id() {
            self.config.refresh_interval_ms = 500;
        } else if event_id == *tray.refresh_1000.id() {
            self.config.refresh_interval_ms = 1000;
        } else if event_id == *tray.refresh_2000.id() {
            self.config.refresh_interval_ms = 2000;
        } else if event_id == tray.quit_id {
            self.save_config();
            event_loop.exit();
            return;
        }

        self.save_config();
        self.sync_tray();
        self.dirty = true;
    }

    fn handle_window_event(
        &mut self,
        window_idx: usize,
        event: WindowEvent,
        event_loop: &ActiveEventLoop,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(inst) = self.windows.get_mut(window_idx) {
                    resize_surface(&mut inst.surface, size);
                    self.dirty = true;
                }
            }
            WindowEvent::RedrawRequested => self.redraw_window(window_idx),
            WindowEvent::Focused(false) => {
                if let Some(inst) = self.windows.get_mut(window_idx) {
                    if inst.dragging {
                        inst.dragging = false;
                        #[cfg(target_os = "linux")]
                        if let Some(x11) = self.x11.as_ref() {
                            x11.ungrab_pointer();
                        }
                        inst.window.set_cursor(winit::window::CursorIcon::Default);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(inst) = self.windows.get_mut(window_idx) {
                    inst.cursor_position = position;
                    if inst.dragging && !self.config.locked {
                        #[cfg(target_os = "linux")]
                        let target_opt = if let Some(x11) = self.x11.as_ref() {
                            if let Some((rx, ry)) = x11.query_pointer() {
                                let dx = rx - inst.drag_start_root.0;
                                let dy = ry - inst.drag_start_root.1;
                                Some((inst.drag_start_win_pos.x + dx, inst.drag_start_win_pos.y + dy))
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        #[cfg(not(target_os = "linux"))]
                        let target_opt: Option<(i32, i32)> = None;

                        if let Some((target_x, target_y)) = target_opt {
                            if target_x != inst.drag_last_pos.x || target_y != inst.drag_last_pos.y {
                                let new_pos = PhysicalPosition::new(target_x, target_y);
                                inst.drag_last_pos = new_pos;
                                inst.window.set_outer_position(new_pos);
                                self.config.position.x = Some(target_x);
                                self.config.position.y = Some(target_y);
                                if self.config.all_screens_top {
                                    self.config.all_screens_top = false;
                                    self.sync_tray();
                                }
                            }
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } if !self.config.mouse_passthrough => {
                let step = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_x, y) => y * 0.05,
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        (pos.y as f32 / 100.0).clamp(-0.1, 0.1) * 0.5
                    }
                };
                if step.abs() > 0.001 {
                    let new_opacity = (self.config.opacity + step).clamp(0.10, 1.0);
                    self.config.opacity = (new_opacity * 100.0).round() / 100.0;
                    self.save_config();
                    self.sync_tray();
                    self.dirty = true;
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } if !self.config.mouse_passthrough => {
                if let Some(inst) = self.windows.get_mut(window_idx) {
                    inst.menu_open = !inst.menu_open;
                }
                self.resize_to_content();
                self.dirty = true;
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => self.left_click(window_idx, event_loop),
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                let mut should_save = false;
                let mut should_sync = false;
                let mut sticky_win = None;
                if let Some(inst) = self.windows.get_mut(window_idx) {
                    if inst.dragging {
                        inst.dragging = false;
                        #[cfg(target_os = "linux")]
                        if let Some(x11) = self.x11.as_ref() {
                            x11.ungrab_pointer();
                        }
                        inst.window.set_cursor(winit::window::CursorIcon::Default);
                        if let Ok(position) = inst.window.outer_position() {
                            self.config.position.x = Some(position.x);
                            self.config.position.y = Some(position.y);
                            if self.config.all_screens_top {
                                let mon = inst
                                    .window
                                    .current_monitor()
                                    .or_else(|| inst.window.primary_monitor());
                                if let Some(m) = mon {
                                    if (position.y - m.position().y).abs() > 40 {
                                        self.config.all_screens_top = false;
                                        should_sync = true;
                                    }
                                }
                            }
                            sticky_win = Some(inst.window.clone());
                            should_save = true;
                        }
                    }
                }
                if let Some(win) = sticky_win {
                    #[cfg(target_os = "linux")]
                    if let Some(x11) = self.x11.as_ref() {
                        x11.apply_sticky(&win);
                    } else {
                        linux_monitor::platform::apply_x11_sticky(&win);
                    }
                }
                if should_sync {
                    self.sync_tray();
                }
                if should_save {
                    self.save_config();
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if let Some(text) = event.text.as_ref() {
                    match text.as_str() {
                        "m" | "M" => {
                            self.config.mouse_passthrough = !self.config.mouse_passthrough;
                            self.save_config();
                            for w in &self.windows {
                                let _ = w.window.set_cursor_hittest(!self.config.mouse_passthrough);
                            }
                            self.sync_tray();
                            self.dirty = true;
                        }
                        "+" | "=" | "]" => {
                            self.config.opacity = (self.config.opacity + 0.05).min(1.0);
                            self.save_config();
                            self.sync_tray();
                            self.dirty = true;
                        }
                        "-" | "_" | "[" => {
                            self.config.opacity = (self.config.opacity - 0.05).max(0.10);
                            self.save_config();
                            self.sync_tray();
                            self.dirty = true;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn left_click(&mut self, window_idx: usize, event_loop: &ActiveEventLoop) {
        let is_menu_open = self.windows.get(window_idx).map(|w| w.menu_open).unwrap_or(false);
        if is_menu_open {
            let line_height = self.line_height();
            let content_h = self.content_height(true);
            let cur_y = self.windows[window_idx].cursor_position.y;
            if let Some(index) = menu_index(cur_y, content_h as f64, line_height as f64) {
                self.activate_menu(index, event_loop);
            } else if let Some(inst) = self.windows.get_mut(window_idx) {
                inst.menu_open = false;
                self.resize_to_content();
                self.dirty = true;
            }
            return;
        }

        if !self.config.locked && !self.config.mouse_passthrough {
            if let Some(inst) = self.windows.get_mut(window_idx) {
                inst.dragging = true;
                if let Ok(pos) = inst.window.outer_position() {
                    inst.drag_start_win_pos = pos;
                    inst.drag_last_pos = pos;
                }
                #[cfg(target_os = "linux")]
                {
                    if let Some(x11) = self.x11.as_ref() {
                        let root_pos = x11.query_pointer().unwrap_or((
                            inst.drag_start_win_pos.x + inst.cursor_position.x.round() as i32,
                            inst.drag_start_win_pos.y + inst.cursor_position.y.round() as i32,
                        ));
                        inst.drag_start_root = root_pos;
                        x11.grab_pointer(&inst.window);
                    } else {
                        let _ = inst.window.drag_window();
                    }
                }
                #[cfg(not(target_os = "linux"))]
                {
                    let _ = inst.window.drag_window();
                }
                inst.window.set_cursor(winit::window::CursorIcon::Move);
            }
        }
    }

    fn activate_menu(&mut self, index: usize, event_loop: &ActiveEventLoop) {
        match index {
            0 => {
                self.config.all_screens_top = !self.config.all_screens_top;
                if self.config.all_screens_top {
                    self.config.always_on_top = true;
                }
                self.reconfigure_windows(event_loop);
            }
            1 => {
                self.config.always_on_top = !self.config.always_on_top;
                self.apply_level_and_sticky();
            }
            2 => {
                self.config.mouse_passthrough = !self.config.mouse_passthrough;
                for w in &self.windows {
                    let _ = w.window.set_cursor_hittest(!self.config.mouse_passthrough);
                }
            }
            3 => self.config.locked = !self.config.locked,
            4 => {
                self.config.cycle_layout();
                self.resize_to_content();
            }
            5 => self.config.cycle_theme(),
            6 => {
                self.config.cycle_style();
                self.resize_to_content();
            }
            7 => {
                self.config.cycle_opacity();
            }
            8 => {
                self.save_config();
                std::process::exit(0);
            }
            _ => return,
        }
        self.save_config();
        self.sync_tray();
        for inst in &mut self.windows {
            inst.menu_open = false;
        }
        self.resize_to_content();
        self.dirty = true;
    }

    fn reconfigure_windows(&mut self, event_loop: &ActiveEventLoop) {
        if self.config.all_screens_top {
            let monitors: Vec<MonitorHandle> = event_loop.available_monitors().collect();
            if monitors.len() > 1 {
                if let Some(first_win) = self.windows.first_mut() {
                    first_win.monitor_name = monitors[0].name();
                    place_window(&first_win.window, &self.config, monitors.first());
                }
                for mon in monitors.iter().skip(1) {
                    let mon_name = mon.name();
                    let exists = self.windows.iter().any(|w| w.monitor_name == mon_name);
                    if !exists {
                        if let Ok(win) = event_loop.create_window(build_window_attributes(&self.config)) {
                            let win = Arc::new(win);
                            setup_window(&win, &self.config);
                            place_window(&win, &self.config, Some(mon));
                            let win_pos = win.outer_position().unwrap_or(PhysicalPosition::new(0, 0));
                            if let Ok(surf) = Surface::new(&self.context, win.clone()) {
                                let instance = WindowInstance {
                                    window: win,
                                    surface: surf,
                                    monitor_name: mon_name,
                                    cursor_position: PhysicalPosition::new(0.0, 0.0),
                                    dragging: false,
                                    drag_start_root: (0, 0),
                                    drag_start_win_pos: win_pos,
                                    drag_last_pos: win_pos,
                                    menu_open: false,
                                    frame_size: PhysicalSize::new(0, 0),
                                    sticky_applied: false,
                                };
                                instance.window.request_redraw();
                                self.windows.push(instance);
                            }
                        }
                    }
                }
            } else if let Some(first_win) = self.windows.first_mut() {
                place_window(&first_win.window, &self.config, monitors.first());
            }

            for inst in &self.windows {
                inst.window.set_window_level(WindowLevel::AlwaysOnTop);
                #[cfg(target_os = "linux")]
                if let Some(x11) = self.x11.as_ref() {
                    x11.apply_sticky(&inst.window);
                } else {
                    linux_monitor::platform::apply_x11_sticky(&inst.window);
                }
            }
        } else {
            if self.windows.len() > 1 {
                self.windows.truncate(1);
            }
            if let Some(first_win) = self.windows.first_mut() {
                first_win.window.set_window_level(if self.config.always_on_top {
                    WindowLevel::AlwaysOnTop
                } else {
                    WindowLevel::Normal
                });
                place_window(&first_win.window, &self.config, None);
                #[cfg(target_os = "linux")]
                if let Some(x11) = self.x11.as_ref() {
                    x11.apply_sticky(&first_win.window);
                } else {
                    linux_monitor::platform::apply_x11_sticky(&first_win.window);
                }
            }
        }
        self.resize_to_content();
        self.sync_tray();
        self.dirty = true;
    }

    fn reposition_windows(&mut self) {
        if self.config.all_screens_top {
            for inst in &self.windows {
                let mon = inst
                    .window
                    .available_monitors()
                    .find(|m| m.name() == inst.monitor_name)
                    .or_else(|| inst.window.primary_monitor());
                place_window(&inst.window, &self.config, mon.as_ref());
            }
        }
        self.dirty = true;
    }

    fn apply_level_and_sticky(&self) {
        let level = if self.config.always_on_top || self.config.all_screens_top {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        };
        for inst in &self.windows {
            inst.window.set_window_level(level);
            #[cfg(target_os = "linux")]
            if let Some(x11) = self.x11.as_ref() {
                x11.apply_sticky(&inst.window);
            } else {
                linux_monitor::platform::apply_x11_sticky(&inst.window);
            }
        }
    }

    fn sync_tray(&self) {
        #[cfg(target_os = "linux")]
        if let Some(tray) = &self.tray {
            let is_visible = self
                .windows
                .first()
                .and_then(|w| w.window.is_visible())
                .unwrap_or(true);
            tray.sync(&self.config, is_visible);
        }
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

    fn redraw_window(&mut self, window_idx: usize) {
        let font = &self.font;
        let state = &self.state;
        let config = &self.config;
        let session = &self.session;
        let Some(inst) = self.windows.get_mut(window_idx) else {
            return;
        };
        let frame = Frame::render(font, state, config, session, inst.menu_open);
        let needed_size = PhysicalSize::new(frame.width, frame.height);
        if inst.frame_size != needed_size {
            inst.frame_size = needed_size;
            let _ = inst.window.request_inner_size(needed_size);
            resize_surface(&mut inst.surface, needed_size);
        }

        if resize_surface(&mut inst.surface, inst.frame_size)
            && let Ok(mut buffer) = inst.surface.buffer_mut()
        {
            let count = buffer.len().min(frame.pixels.len());
            buffer[..count].copy_from_slice(&frame.pixels[..count]);
            if let Err(error) = buffer.present() {
                eprintln!("linux-monitor: 窗口刷新失败: {error}");
            }
        }

        #[cfg(target_os = "linux")]
        let sticky_to_apply = if !inst.sticky_applied {
            inst.sticky_applied = true;
            Some(inst.window.clone())
        } else {
            None
        };
        #[cfg(target_os = "linux")]
        if let Some(win) = sticky_to_apply {
            if let Some(x11) = self.x11.as_ref() {
                x11.apply_sticky(&win);
            } else {
                linux_monitor::platform::apply_x11_sticky(&win);
            }
        }
    }

    fn resize_to_content(&mut self) {
        let blocks = render::metric_blocks(&self.state, &self.config);
        for inst in &mut self.windows {
            let (width, height) =
                render::calculate_size(&blocks, &self.config, inst.menu_open);
            let needed_size = PhysicalSize::new(width, height);
            inst.frame_size = needed_size;
            let _ = inst.window.request_inner_size(needed_size);
            resize_surface(&mut inst.surface, needed_size);
        }
        self.reposition_windows();
    }

    fn line_height(&self) -> u32 {
        (self.config.font_size * 1.45).ceil() as u32
    }

    fn content_height(&self, menu_open: bool) -> u32 {
        let blocks = render::metric_blocks(&self.state, &self.config);
        let (_, height) = render::calculate_size(&blocks, &self.config, menu_open);
        height
    }
}

fn menu_index(cursor_y: f64, content_height: f64, line_height: f64) -> Option<usize> {
    let top = content_height - line_height * MENU_ITEM_COUNT as f64;
    if cursor_y < top {
        return None;
    }
    let index = ((cursor_y - top) / line_height) as usize;
    (index < MENU_ITEM_COUNT).then_some(index)
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
    candidates
}

#[cfg(not(target_os = "windows"))]
fn collect_font_files(directory: &Path, depth: u8, candidates: &mut Vec<PathBuf>) {
    if depth > 2 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_font_files(&path, depth + 1, candidates);
        } else if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
            let extension = extension.to_ascii_lowercase();
            if extension == "ttf" || extension == "otf" {
                candidates.push(path);
            }
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
        let blocks = render::metric_blocks(state, config);
        let (width, height) = render::calculate_size(&blocks, config, menu_open);
        let mut frame = Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize],
        };
        let colors = render::theme_colors(config, is_system_dark(session));
        let radius = match config.window_style {
            WindowStyle::Capsule => height / 2,
            WindowStyle::Flat => 4,
            _ => config.corner_radius.max(8).min(16),
        };

        frame.fill_rounded_rect(0, 0, width, height, radius, colors.background);

        if config.window_style != WindowStyle::Flat && (colors.border >> 24) > 0 {
            frame.stroke_rounded_rect(0, 0, width, height, radius, colors.border);
        }

        let padding_x = match config.window_style {
            WindowStyle::Capsule => (config.font_size * 1.35).ceil() as i32,
            _ => (config.font_size * 1.15).ceil() as i32,
        };
        let padding_y = match config.window_style {
            WindowStyle::Capsule => (config.font_size * 0.55).ceil() as i32,
            _ => (config.font_size * 0.70).ceil() as i32,
        };
        let line_height = (config.font_size * 1.45).ceil() as i32;

        let show_bars = config.window_style == WindowStyle::MiniBar || config.show_mini_bars;
        let bar_width = 34_u32;
        let bar_height = (config.font_size * 0.38).clamp(4.0, 7.0).round() as u32;

        if config.layout == Layout::Horizontal {
            let mut cursor_x = padding_x;
            let text_y = padding_y + (line_height - config.font_size as i32) / 2;
            let total_blocks = blocks.len();

            for (i, block) in blocks.iter().enumerate() {
                let color = render::metric_color(block.kind, state, config, colors);
                cursor_x = frame.draw_text(
                    font,
                    &block.label,
                    cursor_x,
                    text_y,
                    config.font_size,
                    color,
                );

                if show_bars && let Some(pct) = block.percent {
                    cursor_x += 6;
                    let bar_y =
                        text_y + (config.font_size as i32 - bar_height as i32) / 2 + 1;
                    frame.draw_progress_bar(
                        cursor_x,
                        bar_y,
                        bar_width,
                        bar_height,
                        pct,
                        colors.bar_track,
                        color,
                    );
                    cursor_x += bar_width as i32;
                }

                if i + 1 < total_blocks {
                    cursor_x += 7;
                    cursor_x = frame.draw_text(
                        font,
                        "│",
                        cursor_x,
                        text_y,
                        config.font_size,
                        colors.separator,
                    );
                    cursor_x += 7;
                }
            }
        } else {
            let mut y = padding_y;
            for block in &blocks {
                let color = render::metric_color(block.kind, state, config, colors);
                let text_y = y + (line_height - config.font_size as i32) / 2;
                let next_x = frame.draw_text(
                    font,
                    &block.label,
                    padding_x,
                    text_y,
                    config.font_size,
                    color,
                );

                if show_bars && let Some(pct) = block.percent {
                    let bar_x = next_x + 8;
                    let bar_y =
                        text_y + (config.font_size as i32 - bar_height as i32) / 2 + 1;
                    frame.draw_progress_bar(
                        bar_x,
                        bar_y,
                        bar_width,
                        bar_height,
                        pct,
                        colors.bar_track,
                        color,
                    );
                }
                y += line_height;
            }
        }

        if menu_open {
            let menu_y = height as i32 - line_height * MENU_ITEM_COUNT as i32 - 4;
            frame.fill_rect(
                padding_x / 2,
                menu_y - 2,
                width as i32 - padding_x / 2,
                menu_y - 1,
                colors.separator,
            );
            let menu = [
                format!("All screens top: {}", on_off(config.all_screens_top)),
                format!("Always on top: {}", on_off(config.always_on_top)),
                format!("Passthrough: {}", on_off(config.mouse_passthrough)),
                format!("Locked: {}", on_off(config.locked)),
                format!("Layout: {}", layout_name(config.layout)),
                format!("Theme: {}", theme_name(config.theme)),
                format!("Style: {}", style_name(config.window_style)),
                format!("Opacity: {:.0}%", config.opacity * 100.0),
                "Quit".to_string(),
            ];
            for (index, item) in menu.iter().enumerate() {
                frame.draw_text(
                    font,
                    item,
                    padding_x,
                    menu_y + 4
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
        if (color >> 24) == 0 && (color & 0x00_FF_FF_FF) == 0 {
            return;
        }
        let radius = radius.min(width / 2).min(height / 2) as i32;
        let radius_squared = radius * radius;
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let inside_x = (x >= radius) && (x < width as i32 - radius);
                let inside_y = (y >= radius) && (y < height as i32 - radius);
                if inside_x || inside_y {
                    self.blend_pixel(left + x, top + y, color, 255);
                    continue;
                }

                let corner_x = if x < radius {
                    radius
                } else {
                    width as i32 - radius - 1
                };
                let corner_y = if y < radius {
                    radius
                } else {
                    height as i32 - radius - 1
                };
                let dx = x - corner_x;
                let dy = y - corner_y;
                let distance_squared = dx * dx + dy * dy;
                if distance_squared <= radius_squared {
                    self.blend_pixel(left + x, top + y, color, 255);
                }
            }
        }
    }

    fn stroke_rounded_rect(
        &mut self,
        left: i32,
        top: i32,
        width: u32,
        height: u32,
        radius: u32,
        color: u32,
    ) {
        if (color >> 24) == 0 && (color & 0x00_FF_FF_FF) == 0 {
            return;
        }
        let radius = (radius as i32).min(width as i32 / 2).min(height as i32 / 2);
        let r_outer_sq = radius * radius;
        let r_inner = (radius - 1).max(0);
        let r_inner_sq = r_inner * r_inner;

        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let is_top = y == 0;
                let is_bottom = y == height as i32 - 1;
                let is_left = x == 0;
                let is_right = x == width as i32 - 1;

                let in_mid_x = x >= radius && x < width as i32 - radius;
                let in_mid_y = y >= radius && y < height as i32 - radius;

                if (in_mid_x && (is_top || is_bottom)) || (in_mid_y && (is_left || is_right)) {
                    self.blend_pixel(left + x, top + y, color, 255);
                    continue;
                }

                if !in_mid_x && !in_mid_y {
                    let corner_x = if x < radius { radius } else { width as i32 - radius - 1 };
                    let corner_y = if y < radius { radius } else { height as i32 - radius - 1 };
                    let dx = x - corner_x;
                    let dy = y - corner_y;
                    let dist_sq = dx * dx + dy * dy;
                    if dist_sq <= r_outer_sq && dist_sq >= r_inner_sq {
                        self.blend_pixel(left + x, top + y, color, 255);
                    }
                }
            }
        }
    }

    fn draw_progress_bar(
        &mut self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        percent: f32,
        track_color: u32,
        fill_color: u32,
    ) {
        let radius = height / 2;
        self.fill_rounded_rect(x, y, width, height, radius, track_color);
        let fill_w = ((width as f32 * (percent.clamp(0.0, 100.0) / 100.0)).round() as u32).min(width);
        if fill_w > 0 {
            self.fill_rounded_rect(x, y, fill_w, height, radius, fill_color);
        }
    }

    fn draw_text(&mut self, font: &Font, text: &str, x: i32, y: i32, size: f32, color: u32) -> i32 {
        let mut cursor = x;
        for character in text.chars() {
            let (metrics, bitmap) = font.rasterize(character, size);
            let char_x = cursor + metrics.xmin;
            let char_y = y + size as i32 - metrics.height as i32 - metrics.ymin;
            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let alpha = bitmap[row * metrics.width + col];
                    if alpha > 0 {
                        self.blend_pixel(
                            char_x + col as i32,
                            char_y + row as i32,
                            color,
                            alpha,
                        );
                    }
                }
            }
            cursor += metrics.advance_width.ceil() as i32;
        }
        cursor
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
    if value {
        "ON"
    } else {
        "OFF"
    }
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
        Theme::Catppuccin => "Catppuccin",
        Theme::Cyberpunk => "Cyberpunk",
        Theme::Nord => "Nord",
        Theme::Matrix => "Matrix",
        Theme::Solarized => "Solarized",
        Theme::Light => "Light",
        Theme::Transparent => "Transparent",
        Theme::System => "System",
    }
}

fn style_name(style: WindowStyle) -> &'static str {
    match style {
        WindowStyle::Card => "Card (卡片)",
        WindowStyle::Capsule => "Capsule (胶囊)",
        WindowStyle::MiniBar => "MiniBar (进度条)",
        WindowStyle::Flat => "Flat (极简)",
    }
}
