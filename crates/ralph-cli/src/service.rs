//! CLI commands for `ralph service`.
//!
//! Manage Ralph as an OS-level service.
//!
//! Subcommands:
//! - `install`   — Generate and install a service unit file
//! - `uninstall` — Remove the service unit file
//! - `start`     — Start the service
//! - `stop`      — Stop the service
//! - `restart`   — Restart the service
//! - `status`    — Show service status
//! - `enable`    — Enable autostart at boot
//! - `disable`   — Disable autostart at boot
//! - `logs`      — View service logs
//!
//! Supported platforms:
//! - **Linux**: systemd (`systemctl`)
//! - **macOS**: launchd (`launchctl`)

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::Command;

// ─────────────────────────────────────────────────────────────────────────────
// CLI STRUCTS
// ─────────────────────────────────────────────────────────────────────────────

/// Manage Ralph as an OS-level service.
#[derive(Parser, Debug)]
pub struct ServiceArgs {
    #[command(subcommand)]
    pub command: ServiceCommands,
}

#[derive(Subcommand, Debug)]
pub enum ServiceCommands {
    /// Generate and install a service unit file (systemd or launchd)
    Install(InstallArgs),

    /// Remove the installed service unit file
    Uninstall(UninstallArgs),

    /// Start the service
    Start(ServiceNameArgs),

    /// Stop the service
    Stop(ServiceNameArgs),

    /// Restart the service
    Restart(ServiceNameArgs),

    /// Show service status
    Status(ServiceNameArgs),

    /// Enable the service to start at boot
    Enable(ServiceNameArgs),

    /// Disable the service from starting at boot
    Disable(ServiceNameArgs),

    /// View service logs
    Logs(LogsArgs),
}

/// Arguments shared by most service subcommands.
#[derive(Parser, Debug)]
pub struct ServiceNameArgs {
    /// Service name (default: ralph-web)
    #[arg(long, default_value = "ralph-web")]
    pub name: String,
}

/// Arguments for `ralph service install`.
#[derive(Parser, Debug)]
pub struct InstallArgs {
    /// Service name (default: ralph-web)
    #[arg(long, default_value = "ralph-web")]
    pub name: String,

    /// Command that the service runs (default: ralph web)
    #[arg(long, default_value = "ralph web")]
    pub exec: String,

    /// Description shown in the service manager
    #[arg(long, default_value = "Ralph Orchestrator Web Dashboard")]
    pub description: String,

    /// Install as a system-wide service (requires root/sudo). Defaults to user service.
    #[arg(long, conflicts_with = "user")]
    pub system: bool,

    /// Install as a user-level service (default)
    #[arg(long, conflicts_with = "system")]
    pub user: bool,

    /// Print the generated unit file to stdout without installing
    #[arg(long)]
    pub dry_run: bool,
}

/// Arguments for `ralph service uninstall`.
#[derive(Parser, Debug)]
pub struct UninstallArgs {
    /// Service name (default: ralph-web)
    #[arg(long, default_value = "ralph-web")]
    pub name: String,

    /// Uninstall from system-wide location. Defaults to user service.
    #[arg(long, conflicts_with = "user")]
    pub system: bool,

    /// Uninstall from user-level location (default)
    #[arg(long, conflicts_with = "system")]
    pub user: bool,
}

/// Arguments for `ralph service logs`.
#[derive(Parser, Debug)]
pub struct LogsArgs {
    /// Service name (default: ralph-web)
    #[arg(long, default_value = "ralph-web")]
    pub name: String,

    /// Follow log output in real-time
    #[arg(short, long)]
    pub follow: bool,

    /// Number of recent lines to show (default: 50)
    #[arg(short = 'n', long, default_value = "50")]
    pub lines: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// PLATFORM DETECTION
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // MacOs and Unsupported are used on their respective platforms
pub(crate) enum Platform {
    Linux,
    MacOs,
    Unsupported(String),
}

pub(crate) fn detect_platform() -> Platform {
    #[cfg(target_os = "linux")]
    return Platform::Linux;

    #[cfg(target_os = "macos")]
    return Platform::MacOs;

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return Platform::Unsupported(std::env::consts::OS.to_string());
}

// ─────────────────────────────────────────────────────────────────────────────
// SERVICE MANAGER
// ─────────────────────────────────────────────────────────────────────────────

/// Represents the OS-level service manager to use.
#[derive(Debug, Clone)]
pub(crate) enum ServiceManager {
    /// systemd via `systemctl` (Linux)
    Systemd { user: bool },
    /// launchd via `launchctl` (macOS)
    Launchd { user: bool },
}

impl ServiceManager {
    pub(crate) fn new(platform: &Platform, system: bool) -> Result<Self> {
        let user = !system;
        match platform {
            Platform::Linux => Ok(ServiceManager::Systemd { user }),
            Platform::MacOs => Ok(ServiceManager::Launchd { user }),
            Platform::Unsupported(os) => bail!(
                "OS-level service management is not supported on '{}'. \
                 Supported platforms: Linux (systemd), macOS (launchd).",
                os
            ),
        }
    }

    /// Install a service unit file.
    pub(crate) fn install(&self, name: &str, exec: &str, description: &str) -> Result<PathBuf> {
        match self {
            ServiceManager::Systemd { user } => {
                install_systemd(name, exec, description, *user)
            }
            ServiceManager::Launchd { user } => {
                install_launchd(name, exec, description, *user)
            }
        }
    }

    /// Uninstall a service unit file.
    pub(crate) fn uninstall(&self, name: &str) -> Result<()> {
        match self {
            ServiceManager::Systemd { user } => uninstall_systemd(name, *user),
            ServiceManager::Launchd { user } => uninstall_launchd(name, *user),
        }
    }

    /// Delegate a subcommand (start/stop/restart/status/enable/disable) to the service manager.
    pub(crate) fn run_subcommand(&self, subcommand: &str, name: &str) -> Result<()> {
        match self {
            ServiceManager::Systemd { user } => systemctl(subcommand, name, *user),
            ServiceManager::Launchd { .. } => launchctl_subcommand(subcommand, name, self),
        }
    }

    /// View service logs.
    pub(crate) fn logs(&self, name: &str, follow: bool, lines: u32) -> Result<()> {
        match self {
            ServiceManager::Systemd { user } => systemd_logs(name, follow, lines, *user),
            ServiceManager::Launchd { user } => launchd_logs(name, follow, lines, *user),
        }
    }

    /// Return the launchd label for a service name.
    fn launchd_label(&self, name: &str) -> String {
        format!("com.ralph-orchestrator.{}", name)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SYSTEMD HELPERS
// ─────────────────────────────────────────────────────────────────────────────

/// Return the systemd unit file path for `name`.
pub(crate) fn systemd_unit_path(name: &str, user: bool) -> PathBuf {
    if user {
        let base = xdg_config_dir();
        base.join("systemd").join("user").join(format!("{}.service", name))
    } else {
        PathBuf::from(format!("/etc/systemd/system/{}.service", name))
    }
}

/// Generate a systemd unit file content.
pub(crate) fn systemd_unit(exec: &str, description: &str) -> String {
    format!(
        "[Unit]\n\
         Description={description}\n\
         After=network.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={exec}\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        description = description,
        exec = exec,
    )
}

fn install_systemd(name: &str, exec: &str, description: &str, user: bool) -> Result<PathBuf> {
    let unit_path = systemd_unit_path(name, user);
    let content = systemd_unit(exec, description);

    if let Some(parent) = unit_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    std::fs::write(&unit_path, &content)
        .with_context(|| format!("Failed to write service file: {}", unit_path.display()))?;

    // Reload the systemd daemon so it picks up the new unit.
    let daemon_reload_result = systemctl("daemon-reload", "", user);
    if let Err(e) = daemon_reload_result {
        eprintln!("Warning: daemon-reload failed: {}", e);
    }

    Ok(unit_path)
}

fn uninstall_systemd(name: &str, user: bool) -> Result<()> {
    // Stop and disable first (best-effort).
    let _ = systemctl("stop", name, user);
    let _ = systemctl("disable", name, user);

    let unit_path = systemd_unit_path(name, user);
    if unit_path.exists() {
        std::fs::remove_file(&unit_path)
            .with_context(|| format!("Failed to remove: {}", unit_path.display()))?;
    } else {
        bail!("Service unit file not found: {}", unit_path.display());
    }

    let _ = systemctl("daemon-reload", "", user);
    Ok(())
}

fn systemctl(subcommand: &str, name: &str, user: bool) -> Result<()> {
    let mut cmd = Command::new("systemctl");
    if user {
        cmd.arg("--user");
    }
    if subcommand == "daemon-reload" {
        cmd.arg("daemon-reload");
    } else {
        cmd.arg(subcommand).arg(name);
    }

    let status = cmd
        .status()
        .with_context(|| format!("Failed to execute systemctl {}", subcommand))?;

    if !status.success() {
        bail!(
            "systemctl {} {} exited with status {}",
            subcommand,
            name,
            status
        );
    }
    Ok(())
}

fn systemd_logs(name: &str, follow: bool, lines: u32, user: bool) -> Result<()> {
    let mut cmd = Command::new("journalctl");
    if user {
        cmd.arg("--user");
    }
    cmd.arg("-u").arg(name);
    cmd.arg(format!("-n{}", lines));
    if follow {
        cmd.arg("-f");
    }

    let status = cmd
        .status()
        .with_context(|| "Failed to execute journalctl")?;

    if !status.success() {
        bail!("journalctl exited with status {}", status);
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// LAUNCHD HELPERS
// ─────────────────────────────────────────────────────────────────────────────

/// Return the launchd plist path.
pub(crate) fn launchd_plist_path(name: &str, user: bool) -> PathBuf {
    let label = format!("com.ralph-orchestrator.{}", name);
    if user {
        let home = home_dir();
        home.join("Library")
            .join("LaunchAgents")
            .join(format!("{}.plist", label))
    } else {
        PathBuf::from(format!("/Library/LaunchDaemons/{}.plist", label))
    }
}

/// Return the (stdout, stderr) log file paths used by a launchd service.
///
/// These must match the paths written into the plist by [`launchd_plist`].
pub(crate) fn launchd_log_paths(name: &str) -> (String, String) {
    (
        format!("/tmp/{}.stdout.log", name),
        format!("/tmp/{}.stderr.log", name),
    )
}

/// Generate a launchd plist content.
pub(crate) fn launchd_plist(name: &str, exec: &str, _description: &str) -> String {
    let label = format!("com.ralph-orchestrator.{}", name);
    // Split exec into program arguments for plist
    let program_args: Vec<String> = shell_words(exec);
    let args_xml: String = program_args
        .iter()
        .map(|a| format!("        <string>{}</string>\n", xml_escape(a)))
        .collect();

    let (stdout_log, stderr_log) = launchd_log_paths(name);

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\"\n  \
           \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
             <key>Label</key>\n\
             <string>{label}</string>\n\
             <key>ProgramArguments</key>\n\
             <array>\n\
         {args_xml}\
             </array>\n\
             <key>RunAtLoad</key>\n\
             <true/>\n\
             <key>KeepAlive</key>\n\
             <true/>\n\
             <key>StandardOutPath</key>\n\
             <string>{stdout_log}</string>\n\
             <key>StandardErrorPath</key>\n\
             <string>{stderr_log}</string>\n\
         </dict>\n\
         </plist>\n",
        label = label,
        args_xml = args_xml,
        stdout_log = stdout_log,
        stderr_log = stderr_log,
    )
}

fn install_launchd(name: &str, exec: &str, description: &str, user: bool) -> Result<PathBuf> {
    let plist_path = launchd_plist_path(name, user);
    let content = launchd_plist(name, exec, description);

    if let Some(parent) = plist_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    std::fs::write(&plist_path, &content)
        .with_context(|| format!("Failed to write plist: {}", plist_path.display()))?;

    Ok(plist_path)
}

fn uninstall_launchd(name: &str, user: bool) -> Result<()> {
    let plist_path = launchd_plist_path(name, user);

    // Unload first (best-effort).
    let _ = Command::new("launchctl")
        .args(["unload", &plist_path.display().to_string()])
        .status();

    if plist_path.exists() {
        std::fs::remove_file(&plist_path)
            .with_context(|| format!("Failed to remove: {}", plist_path.display()))?;
    } else {
        bail!("Plist not found: {}", plist_path.display());
    }

    Ok(())
}

fn launchctl_subcommand(subcommand: &str, name: &str, manager: &ServiceManager) -> Result<()> {
    let label = manager.launchd_label(name);

    // Determine the launchctl target domain based on user/system scope
    let domain = match manager {
        ServiceManager::Launchd { user: true } => format!("gui/{}", get_uid()),
        ServiceManager::Launchd { user: false } => "system".to_string(),
        _ => format!("gui/{}", get_uid()), // fallback for non-Launchd (shouldn't occur)
    };

    let launchctl_cmd = match subcommand {
        "start" => ("kickstart", format!("{}/{}", domain, label)),
        "stop" => ("kill", format!("SIGTERM {}/{}", domain, label)),
        "restart" => {
            // stop then start
            let _ = launchctl_subcommand("stop", name, manager);
            return launchctl_subcommand("start", name, manager);
        }
        "status" => ("print", format!("{}/{}", domain, label)),
        "enable" => ("enable", format!("{}/{}", domain, label)),
        "disable" => ("disable", format!("{}/{}", domain, label)),
        other => bail!("Unsupported launchctl subcommand: {}", other),
    };

    let status = Command::new("launchctl")
        .arg(launchctl_cmd.0)
        .arg(&launchctl_cmd.1)
        .status()
        .with_context(|| format!("Failed to execute launchctl {}", subcommand))?;

    if !status.success() {
        bail!(
            "launchctl {} {} exited with status {}",
            launchctl_cmd.0,
            label,
            status
        );
    }
    Ok(())
}

fn launchd_logs(name: &str, follow: bool, lines: u32, _user: bool) -> Result<()> {
    // launchd writes to files configured in the plist (same paths as launchd_log_paths)
    let (stdout_log, stderr_log) = launchd_log_paths(name);

    if follow {
        let status = Command::new("tail")
            .arg("-f")
            .arg(&stdout_log)
            .arg(&stderr_log)
            .status()
            .with_context(|| "Failed to execute tail -f")?;
        if !status.success() {
            bail!("tail exited with status {}", status);
        }
    } else {
        for log_path in &[&stdout_log, &stderr_log] {
            if std::path::Path::new(log_path).exists() {
                println!("==> {} <==", log_path);
                let status = Command::new("tail")
                    .arg(format!("-n{}", lines))
                    .arg(log_path)
                    .status()
                    .with_context(|| format!("Failed to read {}", log_path))?;
                if !status.success() {
                    bail!("tail exited with status {}", status);
                }
            } else {
                println!("(log file not found: {})", log_path);
            }
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// UTILITIES
// ─────────────────────────────────────────────────────────────────────────────

/// Very simple shell word splitter (handles quoted strings and spaces).
fn shell_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;

    for ch in input.chars() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            ' ' if !in_single && !in_double => {
                if !current.is_empty() {
                    words.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Minimal XML character escaping for plist values.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Get the current user's UID (for launchctl domain targeting).
fn get_uid() -> String {
    #[cfg(unix)]
    {
        Command::new("id")
            .arg("-u")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            // Fallback: try the UID env var (set by some shells), otherwise use "501"
            // which is the default first-user UID on macOS (and common on Linux).
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                std::env::var("UID").unwrap_or_else(|_| "501".to_string())
            })
    }
    #[cfg(not(unix))]
    {
        "0".to_string()
    }
}

/// Return the user's home directory.
fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // $HOME is not set; fall back to /tmp to avoid silently using a bad path
            PathBuf::from("/tmp")
        })
}

/// Return the XDG config directory (Linux) or equivalent.
fn xdg_config_dir() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join(".config"))
}

// ─────────────────────────────────────────────────────────────────────────────
// DISPATCHER
// ─────────────────────────────────────────────────────────────────────────────

pub async fn execute(args: ServiceArgs, use_colors: bool) -> Result<()> {
    let platform = detect_platform();

    match args.command {
        ServiceCommands::Install(install_args) => {
            install_command(install_args, &platform, use_colors)
        }
        ServiceCommands::Uninstall(uninstall_args) => {
            uninstall_command(uninstall_args, &platform, use_colors)
        }
        ServiceCommands::Start(a) => delegate_command("start", &a.name, &platform),
        ServiceCommands::Stop(a) => delegate_command("stop", &a.name, &platform),
        ServiceCommands::Restart(a) => delegate_command("restart", &a.name, &platform),
        ServiceCommands::Status(a) => delegate_command("status", &a.name, &platform),
        ServiceCommands::Enable(a) => delegate_command("enable", &a.name, &platform),
        ServiceCommands::Disable(a) => delegate_command("disable", &a.name, &platform),
        ServiceCommands::Logs(logs_args) => logs_command(logs_args, &platform, use_colors),
    }
}

fn install_command(args: InstallArgs, platform: &Platform, use_colors: bool) -> Result<()> {
    let system = args.system;

    if args.dry_run {
        match platform {
            Platform::Linux => {
                let content = systemd_unit(&args.exec, &args.description);
                println!("{}", content);
            }
            Platform::MacOs => {
                let content = launchd_plist(&args.name, &args.exec, &args.description);
                println!("{}", content);
            }
            Platform::Unsupported(os) => {
                bail!(
                    "OS-level service management is not supported on '{}'. \
                     Supported platforms: Linux (systemd), macOS (launchd).",
                    os
                );
            }
        }
        return Ok(());
    }

    let manager = ServiceManager::new(platform, system)?;
    let unit_path = manager.install(&args.name, &args.exec, &args.description)?;

    print_success(
        use_colors,
        &format!(
            "Service '{}' installed at {}",
            args.name,
            unit_path.display()
        ),
    );

    match platform {
        Platform::Linux => {
            println!();
            println!("To start and enable the service:");
            println!(
                "  ralph service start --name {}",
                args.name
            );
            println!(
                "  ralph service enable --name {}",
                args.name
            );
            println!();
            println!("Or use systemctl directly:");
            let user_flag = if !system { " --user" } else { "" };
            println!("  systemctl{} start {}", user_flag, args.name);
            println!("  systemctl{} enable {}", user_flag, args.name);
        }
        Platform::MacOs => {
            let plist_path = launchd_plist_path(&args.name, !system);
            println!();
            println!("To load the service now:");
            println!("  launchctl load {}", plist_path.display());
            println!();
            println!("Or use ralph service start:");
            println!("  ralph service start --name {}", args.name);
        }
        Platform::Unsupported(_) => {}
    }

    Ok(())
}

fn uninstall_command(args: UninstallArgs, platform: &Platform, use_colors: bool) -> Result<()> {
    let system = args.system;
    let manager = ServiceManager::new(platform, system)?;
    manager.uninstall(&args.name)?;

    print_success(
        use_colors,
        &format!("Service '{}' uninstalled.", args.name),
    );
    Ok(())
}

/// Delegate start/stop/restart/status/enable/disable to the service manager (default: user service).
fn delegate_command(subcommand: &str, name: &str, platform: &Platform) -> Result<()> {
    // Default to user service for start/stop/restart/status/enable/disable
    let manager = ServiceManager::new(platform, false)?;
    manager.run_subcommand(subcommand, name)
}

fn logs_command(args: LogsArgs, platform: &Platform, _use_colors: bool) -> Result<()> {
    // Default to user service
    let manager = ServiceManager::new(platform, false)?;
    manager.logs(&args.name, args.follow, args.lines)
}

fn print_success(use_colors: bool, msg: &str) {
    if use_colors {
        println!("\x1b[32m✓\x1b[0m {}", msg);
    } else {
        println!("✓ {}", msg);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TESTS
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── shell_words ───────────────────────────────────────────────────────────

    #[test]
    fn test_shell_words_simple() {
        assert_eq!(
            shell_words("ralph web"),
            vec!["ralph".to_string(), "web".to_string()]
        );
    }

    #[test]
    fn test_shell_words_with_flag() {
        assert_eq!(
            shell_words("ralph web --no-open"),
            vec!["ralph".to_string(), "web".to_string(), "--no-open".to_string()]
        );
    }

    #[test]
    fn test_shell_words_quoted() {
        assert_eq!(
            shell_words(r#"ralph run -p "my prompt""#),
            vec![
                "ralph".to_string(),
                "run".to_string(),
                "-p".to_string(),
                "my prompt".to_string(),
            ]
        );
    }

    #[test]
    fn test_shell_words_single_word() {
        assert_eq!(shell_words("ralph"), vec!["ralph".to_string()]);
    }

    #[test]
    fn test_shell_words_consecutive_spaces_ignored() {
        assert_eq!(
            shell_words("ralph   web"),
            vec!["ralph".to_string(), "web".to_string()]
        );
    }

    #[test]
    fn test_shell_words_single_quoted() {
        assert_eq!(
            shell_words("ralph run -p 'my prompt'"),
            vec![
                "ralph".to_string(),
                "run".to_string(),
                "-p".to_string(),
                "my prompt".to_string(),
            ]
        );
    }

    #[test]
    fn test_shell_words_empty_input() {
        let result = shell_words("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_shell_words_unclosed_quote_included() {
        // Unclosed quotes: the content up to end-of-string is captured
        let result = shell_words(r#"ralph "unclosed"#);
        assert_eq!(result, vec!["ralph".to_string(), "unclosed".to_string()]);
    }

    // ── xml_escape ────────────────────────────────────────────────────────────

    #[test]
    fn test_xml_escape_no_special_chars() {
        assert_eq!(xml_escape("ralph web"), "ralph web");
    }

    #[test]
    fn test_xml_escape_ampersand() {
        assert_eq!(xml_escape("foo & bar"), "foo &amp; bar");
    }

    #[test]
    fn test_xml_escape_angle_brackets() {
        assert_eq!(xml_escape("<value>"), "&lt;value&gt;");
    }

    // ── systemd_unit ─────────────────────────────────────────────────────────

    #[test]
    fn test_systemd_unit_contains_exec() {
        let unit = systemd_unit("ralph web", "Ralph Web");
        assert!(unit.contains("ExecStart=ralph web"));
        assert!(unit.contains("Description=Ralph Web"));
        assert!(unit.contains("[Unit]"));
        assert!(unit.contains("[Service]"));
        assert!(unit.contains("[Install]"));
    }

    #[test]
    fn test_systemd_unit_restart_policy() {
        let unit = systemd_unit("ralph web", "Ralph Web");
        assert!(unit.contains("Restart=on-failure"));
    }

    // ── launchd_plist ─────────────────────────────────────────────────────────

    #[test]
    fn test_launchd_plist_contains_label() {
        let plist = launchd_plist("ralph-web", "ralph web", "Ralph Web");
        assert!(plist.contains("com.ralph-orchestrator.ralph-web"));
    }

    #[test]
    fn test_launchd_plist_contains_program_args() {
        let plist = launchd_plist("ralph-web", "ralph web", "Ralph Web");
        assert!(plist.contains("<string>ralph</string>"));
        assert!(plist.contains("<string>web</string>"));
    }

    #[test]
    fn test_launchd_plist_run_at_load() {
        let plist = launchd_plist("ralph-web", "ralph web", "Ralph Web");
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("<true/>"));
    }

    #[test]
    fn test_launchd_plist_keep_alive() {
        let plist = launchd_plist("ralph-web", "ralph web", "Ralph Web");
        assert!(plist.contains("<key>KeepAlive</key>"));
    }

    // ── systemd_unit_path ─────────────────────────────────────────────────────

    #[test]
    fn test_systemd_unit_path_system() {
        let path = systemd_unit_path("ralph-web", false);
        assert_eq!(path, PathBuf::from("/etc/systemd/system/ralph-web.service"));
    }

    #[test]
    fn test_systemd_unit_path_user() {
        let path = systemd_unit_path("ralph-web", true);
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("systemd/user/ralph-web.service"));
    }

    // ── launchd_plist_path ────────────────────────────────────────────────────

    #[test]
    fn test_launchd_plist_path_system() {
        let path = launchd_plist_path("ralph-web", false);
        assert_eq!(
            path,
            PathBuf::from("/Library/LaunchDaemons/com.ralph-orchestrator.ralph-web.plist")
        );
    }

    #[test]
    fn test_launchd_plist_path_user() {
        let path = launchd_plist_path("ralph-web", true);
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("LaunchAgents"));
        assert!(path_str.contains("com.ralph-orchestrator.ralph-web.plist"));
    }

    // ── ServiceManager ────────────────────────────────────────────────────────

    #[test]
    fn test_service_manager_unsupported_platform() {
        let result =
            ServiceManager::new(&Platform::Unsupported("windows".to_string()), false);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not supported on 'windows'"));
    }

    #[test]
    fn test_service_manager_linux_user() {
        let mgr = ServiceManager::new(&Platform::Linux, false).unwrap();
        assert!(matches!(mgr, ServiceManager::Systemd { user: true }));
    }

    #[test]
    fn test_service_manager_linux_system() {
        let mgr = ServiceManager::new(&Platform::Linux, true).unwrap();
        assert!(matches!(mgr, ServiceManager::Systemd { user: false }));
    }

    #[test]
    fn test_service_manager_macos_user() {
        let mgr = ServiceManager::new(&Platform::MacOs, false).unwrap();
        assert!(matches!(mgr, ServiceManager::Launchd { user: true }));
    }

    // ── install dry-run (no filesystem writes) ────────────────────────────────

    #[test]
    fn test_install_command_dry_run_linux_prints_unit() {
        let args = InstallArgs {
            name: "ralph-web".to_string(),
            exec: "ralph web".to_string(),
            description: "Ralph Web".to_string(),
            system: false,
            user: false,
            dry_run: true,
        };
        // Should not error on Linux or macOS (or at all for dry_run)
        let result = install_command(args, &Platform::Linux, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_install_command_dry_run_macos_prints_plist() {
        let args = InstallArgs {
            name: "ralph-web".to_string(),
            exec: "ralph web".to_string(),
            description: "Ralph Web".to_string(),
            system: false,
            user: false,
            dry_run: true,
        };
        let result = install_command(args, &Platform::MacOs, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_install_command_dry_run_unsupported_errors() {
        let args = InstallArgs {
            name: "ralph-web".to_string(),
            exec: "ralph web".to_string(),
            description: "Ralph Web".to_string(),
            system: false,
            user: false,
            dry_run: true,
        };
        let result =
            install_command(args, &Platform::Unsupported("freebsd".to_string()), false);
        assert!(result.is_err());
    }

    // ── launchd_log_paths consistency ─────────────────────────────────────────

    #[test]
    fn test_launchd_log_paths_match_plist() {
        let name = "ralph-web";
        let (stdout, stderr) = launchd_log_paths(name);
        let plist = launchd_plist(name, "ralph web", "Ralph Web");
        // The paths embedded in the plist must match what launchd_log_paths returns
        assert!(plist.contains(&stdout), "stdout log path must be in plist");
        assert!(plist.contains(&stderr), "stderr log path must be in plist");
    }

    #[test]
    fn test_launchd_log_paths_format() {
        let (stdout, stderr) = launchd_log_paths("my-service");
        assert_eq!(stdout, "/tmp/my-service.stdout.log");
        assert_eq!(stderr, "/tmp/my-service.stderr.log");
    }
}
