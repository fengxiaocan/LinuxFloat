# linux-monitor

Linux 原生轻量性能监控悬浮窗，直接读取 `/proc`、`/sys` 和 NVIDIA NVML，不启动 shell、`nvidia-smi` 或其他常驻监控进程。

## 当前实现

- CPU 总使用率、Linux 实际可用内存占用
- 自动过滤虚拟网卡的下载/上传速度，3 次采样平滑
- CPU 温度缓存选择（Package / Tctl / Tdie 优先）
- AMD GPU sysfs 采集，NVIDIA 动态加载 NVML，Intel sysfs 探测
- **4 款视觉窗口样式**：现代卡片 (`card`)、胶囊药丸 (`capsule`)、动态进度条 (`minibar`)、极简无界 (`flat`)
- **8 大主题配色**：深色 (`dark`)、猫咖柔和 (`catppuccin`)、赛博朋克霓虹 (`cyberpunk`)、北欧极光 (`nord`)、黑客极客绿 (`matrix`)、经典日光 (`solarized`)、明亮浅色 (`light`)、纯粹透明 (`transparent`)、跟随系统 (`system`)
- **一直置顶于所有屏幕顶部**：支持跨显示器多窗吸附、多工作区置顶粘附（Sticky / Always On Top），以及居中/靠右/靠左对齐切换
- 各硬件指标独立主题强调色（网络/CPU/RAM/GPU）与高负载警示变色
- 横向/纵向显示、窗口透明度、圆角调节、指标项自定义开关
- 无边框悬浮窗、拖动、坐标持久化、显示器断开后的坐标恢复
- 鼠标穿透、锁定位置、右键菜单和 Linux KSNI 系统托盘（含二级分类子菜单与状态同步）
- X11 / Wayland 普通窗口路径，wlroots 环境会显示 Layer Shell 能力提示
- `~/.config/linux-monitor/config.toml` 配置和 XDG autostart

## 构建

```bash
cargo build --release
./target/release/linux-monitor
```

## 打包为 Debian 软件包 (.deb)

本项目内置了一键 deb 打包脚本 `build-deb.sh`，会自动完成 release 编译、资源准备、权限规范化与打包校验：

```bash
./build-deb.sh
```

打包生成的文件位于 `dist/` 目录下（如 `dist/linux-monitor_0.1.0_amd64.deb`）。

### 安装与卸载

```bash
# 安装生成的 deb 包
sudo dpkg -i dist/linux-monitor_*.deb

# 如缺少推荐字体等可选依赖，可自动补齐：
sudo apt-get install -f

# 卸载
sudo apt-get remove linux-monitor
```

首次启动默认位于主显示器右上角。右键悬浮窗或系统托盘可以切换鼠标穿透、锁定、置顶、布局、样式和主题；开启鼠标穿透后可通过托盘菜单恢复交互。

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
theme = "cyberpunk"           # dark, catppuccin, cyberpunk, nord, matrix, solarized, light, transparent, system
window_style = "minibar"      # card, capsule, minibar, flat
all_screens_top = true        # 一直显示在所有屏幕顶部
top_alignment = "center"      # center, right, left
show_mini_bars = true         # 在 CPU/RAM/GPU 旁显示实时彩色动态进度条
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
