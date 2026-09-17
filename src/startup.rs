use crate::config::Config;
#[cfg(target_os = "linux")]
use crate::config::ensure_parent;
use std::env;
#[cfg(target_os = "linux")]
use std::fs;
use std::io;
use std::path::PathBuf;

pub fn autostart_path() -> Option<PathBuf> {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config_home.join("autostart").join("linux-monitor.desktop"))
}

pub fn set_autostart(enabled: bool, executable: Option<PathBuf>) -> io::Result<()> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (enabled, executable);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "当前平台暂未实现 XDG 自动启动",
        ))
    }

    #[cfg(target_os = "linux")]
    {
        let path = autostart_path()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "无法定位 XDG 配置目录"))?;
        if enabled {
            let executable = executable
                .or_else(|| env::current_exe().ok())
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "无法定位程序路径"))?;
            ensure_parent(&path)?;
            let content = desktop_entry(&executable);
            let temporary = path.with_extension("desktop.tmp");
            fs::write(&temporary, content)?;
            fs::rename(temporary, path)
        } else if path.exists() {
            fs::remove_file(path)
        } else {
            Ok(())
        }
    }
}

pub fn sync_with_config(config: &Config, executable: Option<PathBuf>) -> io::Result<()> {
    set_autostart(config.autostart, executable)
}

#[cfg(target_os = "linux")]
fn desktop_entry(executable: &std::path::Path) -> String {
    let escaped = executable.to_string_lossy().replace('"', "\\\"");
    format!(
        "[Desktop Entry]\nType=Application\nName=Linux Monitor\nComment=Lightweight performance overlay\nExec=\"{escaped}\"\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    )
}
