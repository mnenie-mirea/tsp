use std::path::PathBuf;

pub const LABEL: &str = "com.editor.monitor";

pub fn plist_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

pub fn plist_content(binary: &str, log_out: &str, log_err: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>run</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{log_out}</string>
    <key>StandardErrorPath</key>
    <string>{log_err}</string>
    <key>ProcessType</key>
    <string>Background</string>
</dict>
</plist>
"#
    )
}

pub fn install() -> anyhow::Result<()> {
    let binary = std::env::current_exe()?;
    let data = crate::config::MonitorConfig::data_dir();
    std::fs::create_dir_all(&data)?;
    let out = data.join("service.out.log");
    let err = data.join("service.err.log");
    std::fs::write(plist_path(), plist_content(&binary.display().to_string(), &out.display().to_string(), &err.display().to_string()))?;
    run_launchctl(&["load", &plist_path().display().to_string()])
}

pub fn uninstall() -> anyhow::Result<()> {
    let _ = run_launchctl(&["unload", &plist_path().display().to_string()]);
    let _ = std::fs::remove_file(plist_path());
    Ok(())
}

/// Отключает автозапуск, но оставляет plist (-w пишет флаг Disabled).
pub fn disable() -> anyhow::Result<()> {
    run_launchctl(&["unload", "-w", &plist_path().display().to_string()])
}

pub fn enable() -> anyhow::Result<()> {
    run_launchctl(&["load", "-w", &plist_path().display().to_string()])
}

pub fn status() -> &'static str {
    if !plist_path().exists() {
        return "не установлена";
    }
    match run_launchctl(&["list", LABEL]) {
        Ok(_) => "запущена (launchd)",
        Err(_) => "установлена, но не запущена",
    }
}

fn run_launchctl(args: &[&str]) -> anyhow::Result<()> {
    let out = std::process::Command::new("launchctl").args(args).output()?;
    if out.status.success() {
        Ok(())
    } else {
        anyhow::bail!(
            "launchctl {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        )
    }
}
