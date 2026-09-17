# linux-monitor

Linux 原生轻量性能监控悬浮窗，直接读取 `/proc`、`/sys` 和 NVIDIA NVML，不启动 shell、`nvidia-smi` 或其他常驻监控进程。

## 当前实现

- CPU 总使用率、Linux 实际可用内存占用
- 自动过滤虚拟网卡的下载/上传速度，3 次采样平滑
- CPU 温度缓存选择（Package / Tctl / Tdie 优先）
- AMD GPU sysfs 采集，NVIDIA 动态加载 NVML，Intel sysfs 探测
- 横向/纵向显示、深色/浅色/透明主题、透明度、阈值颜色
- 无边框悬浮窗、置顶、拖动、坐标持久化、显示器断开后的坐标恢复
- 鼠标穿透、锁定位置、右键菜单和 Linux KSNI 系统托盘
- X11 / Wayland 普通窗口路径，wlroots 环境会显示 Layer Shell 能力提示
- `~/.config/linux-monitor/config.toml` 配置和 XDG autostart

## 构建

```bash
cargo build --release
./target/release/linux-monitor
```

首次启动默认位于主显示器右上角。右键悬浮窗或系统托盘可以切换鼠标穿透、锁定、置顶、布局和主题；开启鼠标穿透后可通过托盘菜单恢复交互。

命令行检查：

```bash
linux-monitor --version
linux-monitor --print-session
linux-monitor --print-config
linux-monitor --no-window
```

## 配置文件示例

```toml
refresh_interval_ms = 1000
layout = "horizontal"
theme = "dark"
opacity = 0.85
always_on_top = true
mouse_passthrough = false
locked = false
autostart = false
```

## 验证

```bash
cargo fmt --check
cargo test
cargo check --target x86_64-unknown-linux-gnu
```

当前 Windows 开发机只负责编译检查，Linux GUI 需要在有 X11 或 Wayland 会话的 Linux 主机上运行。Layer Shell 原生协议尚未接入，wlroots 环境目前使用 winit 的 Wayland 普通窗口路径；若桌面没有可用的 D-Bus 托盘服务，右键菜单仍可作为回退入口。
