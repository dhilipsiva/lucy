//! Explicit service enrollment. Installing a CLI never invokes this module.
#[cfg(unix)]
pub fn command(args: &[String]) -> Result<serde_json::Value, String> {
    use std::path::PathBuf;
    use std::process::Command;
    let action = args
        .first()
        .map(String::as_str)
        .ok_or("use service install or uninstall")?;
    if !matches!(action, "install" | "uninstall") {
        return Err("use service install or uninstall".into());
    }
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME required")?);
    let binary = std::env::current_exe().map_err(|e| e.to_string())?;
    fn run(name: &str, args: &[&str]) -> Result<(), String> {
        let result = Command::new(name)
            .args(args)
            .status()
            .map_err(|e| e.to_string())?;
        if result.success() {
            Ok(())
        } else {
            Err(format!("{name} failed: {result}"))
        }
    }
    #[cfg(target_os = "linux")]
    let (path, text) = {
        let executable = binary
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%");
        (
            home.join(".config/systemd/user/lucy.service"),
            format!(
                "# Managed by Lucy\n[Unit]\nDescription=Lucy private memory service\n\n[Service]\nExecStart=\"{executable}\" serve\nRestart=on-failure\nUMask=0077\n\n[Install]\nWantedBy=default.target\n"
            ),
        )
    };
    #[cfg(target_os = "macos")]
    let (path, text) = {
        let escape = |s: &str| {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
        };
        (
            home.join("Library/LaunchAgents/dev.dhilipsiva.lucy.plist"),
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- Managed by Lucy -->\n<plist version=\"1.0\"><dict><key>Label</key><string>dev.dhilipsiva.lucy</string><key>ProgramArguments</key><array><string>{}</string><string>serve</string></array><key>RunAtLoad</key><true/><key>KeepAlive</key><true/></dict></plist>\n",
                escape(&binary.to_string_lossy())
            ),
        )
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return Err("service enrollment supports Linux and macOS".into());
    if action == "install" {
        if path.exists() {
            return Err("existing service is already managed; update it through its current package/configuration".into());
        }
        // Only the service file needs private mode; do not change shared config directories.
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
    } else if !std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())?
        .contains("Managed by Lucy")
    {
        return Err("refusing to remove a service managed elsewhere".into());
    }
    #[cfg(target_os = "linux")]
    {
        if action == "uninstall" {
            run("systemctl", &["--user", "disable", "--now", "lucy.service"])?;
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        run("systemctl", &["--user", "daemon-reload"])?;
        if action == "install" {
            run("systemctl", &["--user", "enable", "--now", "lucy.service"])?;
        }
    }
    #[cfg(target_os = "macos")]
    {
        let domain = format!("gui/{}", nix::unistd::getuid());
        run(
            "launchctl",
            &[
                if action == "install" {
                    "bootstrap"
                } else {
                    "bootout"
                },
                &domain,
                &path.to_string_lossy(),
            ],
        )?;
        if action == "uninstall" {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(serde_json::json!({"service":action,"path":path}))
}
#[cfg(not(unix))]
pub fn command(_args: &[String]) -> Result<serde_json::Value, String> {
    Err("this release provides the Windows CLI; service enrollment supports Linux and macOS".into())
}
