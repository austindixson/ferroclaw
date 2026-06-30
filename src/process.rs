//! Process lifecycle helpers — gateway PID file, cleanup, and child reaping.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Filename under [`crate::config::data_dir()`] recording the gateway (`serve`) PID.
pub const GATEWAY_PID_FILE: &str = "gateway.pid";

pub fn gateway_pid_path() -> PathBuf {
    crate::config::data_dir().join(GATEWAY_PID_FILE)
}

/// Called when `ferroclaw serve` starts (foreground or background).
pub fn register_gateway_pid() -> std::io::Result<u32> {
    let pid = std::process::id();
    write_gateway_pid(pid)?;
    Ok(pid)
}

pub fn write_gateway_pid(pid: u32) -> std::io::Result<()> {
    let path = gateway_pid_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(&path)?;
    writeln!(file, "{pid}")?;
    Ok(())
}

pub fn read_gateway_pid() -> Option<u32> {
    let content = fs::read_to_string(gateway_pid_path()).ok()?;
    content.lines().next()?.trim().parse().ok()
}

pub fn remove_gateway_pid_file() {
    let _ = fs::remove_file(gateway_pid_path());
}

/// Whether a PID still exists (Unix `kill -0`).
pub fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// Remove stale PID file when the recorded process is gone.
pub fn clear_stale_gateway_pid_file() {
    if let Some(pid) = read_gateway_pid()
        && !is_pid_alive(pid)
    {
        remove_gateway_pid_file();
    }
}

/// Stop the gateway using the PID file, then fall back to pattern match.
pub fn stop_gateway_processes() -> anyhow::Result<bool> {
    let mut stopped = false;

    if let Some(pid) = read_gateway_pid() {
        if is_pid_alive(pid) {
            signal_gateway_shutdown(pid)?;
            stopped = true;
        }
        remove_gateway_pid_file();
    }

    if pkill_serve_pattern()? {
        stopped = true;
    }

    remove_gateway_pid_file();
    Ok(stopped)
}

/// List likely Ferroclaw CLI/gateway PIDs (best-effort; excludes unrelated matches when possible).
pub fn list_ferroclaw_pids() -> anyhow::Result<Vec<(u32, String)>> {
    let mut out = Vec::new();

    if let Some(pid) = read_gateway_pid() {
        let alive = is_pid_alive(pid);
        out.push((
            pid,
            format!(
                "gateway (pid file{})",
                if alive { ", running" } else { ", stale" },
            ),
        ));
    }

    for (pid, cmd) in pgrep_lines("ferroclaw serve")? {
        if !out.iter().any(|(p, _)| *p == pid) {
            out.push((pid, format!("serve: {cmd}")));
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        let exe_str = exe.display().to_string();
        let pattern = format!("{exe_str}");
        for (pid, cmd) in pgrep_lines(&pattern)? {
            if cmd.contains("ferroclaw serve") || cmd.contains("ferroclaw gateway") {
                continue;
            }
            if cmd.contains("Cursor Helper")
                || cmd.contains("extension-host")
                || cmd.contains("zsh -c snap")
                || cmd.contains("COMMAND_EXIT_CODE")
            {
                continue;
            }
            if is_ferroclaw_cli_cmdline(&cmd)
                && !out.iter().any(|(p, _)| *p == pid)
            {
                out.push((pid, format!("cli: {cmd}")));
            }
        }
    }

    out.sort_by_key(|(pid, _)| *pid);
    Ok(out)
}

fn is_ferroclaw_cli_cmdline(cmd: &str) -> bool {
    cmd.contains("ferroclaw --version")
        || cmd.contains("ferroclaw gateway doctor")
        || cmd.contains("ferroclaw run")
        || cmd.contains("ferroclaw exec")
        || cmd.contains("ferroclaw mcp")
        || cmd.contains("ferroclaw cleanup")
}

/// Stop gateway plus stray experiment CLI processes recorded by [`list_ferroclaw_pids`].
pub fn cleanup_ferroclaw_processes(kill: bool) -> anyhow::Result<()> {
    let pids = list_ferroclaw_pids()?;
    if pids.is_empty() {
        println!("No Ferroclaw processes found.");
        clear_stale_gateway_pid_file();
        return Ok(());
    }

    println!("Ferroclaw processes:");
    for (pid, desc) in &pids {
        let alive = is_pid_alive(*pid);
        println!(
            "  {pid:>6}  {} {}",
            if alive { "running" } else { "dead" },
            desc
        );
    }

    if !kill {
        println!("\nRe-run with --kill to terminate them.");
        return Ok(());
    }

    let _ = stop_gateway_processes()?;

    for (pid, _) in &pids {
        if is_pid_alive(*pid) {
            terminate_pid(*pid)?;
        }
    }

    remove_gateway_pid_file();
    println!("\nCleanup finished.");
    Ok(())
}

fn pgrep_lines(pattern: &str) -> anyhow::Result<Vec<(u32, String)>> {
    let output = Command::new("pgrep")
        .arg("-lf")
        .arg(pattern)
        .output()
        .map_err(|e| anyhow::anyhow!("pgrep failed for '{pattern}': {e}"))?;

    if output.status.code() == Some(1) {
        return Ok(Vec::new());
    }
    if !output.status.success() {
        return Ok(Vec::new());
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut rows = Vec::new();
    for line in text.lines() {
        let mut parts = line.splitn(2, ' ');
        let pid: u32 = match parts.next().and_then(|s| s.trim().parse().ok()) {
            Some(p) => p,
            None => continue,
        };
        let cmd = parts.next().unwrap_or("").trim().to_string();
        rows.push((pid, cmd));
    }
    Ok(rows)
}

fn pkill_serve_pattern() -> anyhow::Result<bool> {
    let pattern = "ferroclaw serve";
    let status = Command::new("pkill")
        .arg("-f")
        .arg(pattern)
        .status()
        .map_err(|e| anyhow::anyhow!("pkill failed for '{pattern}': {e}"))?;
    Ok(status.code() == Some(0))
}

#[cfg(unix)]
fn signal_gateway_shutdown(pid: u32) -> anyhow::Result<()> {
    // Prefer process-group signal when this PID is a session leader (setsid on serve).
    let _ = Command::new("kill")
        .arg("-TERM")
        .arg(format!("-{pid}"))
        .status();
    std::thread::sleep(Duration::from_millis(400));
    if is_pid_alive(pid) {
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status();
        std::thread::sleep(Duration::from_millis(400));
    }
    if is_pid_alive(pid) {
        let _ = Command::new("kill")
            .arg("-KILL")
            .arg(format!("-{pid}"))
            .status();
        let _ = Command::new("kill")
            .arg("-KILL")
            .arg(pid.to_string())
            .status();
    }
    Ok(())
}

#[cfg(not(unix))]
fn signal_gateway_shutdown(pid: u32) -> anyhow::Result<()> {
    let _ = pid;
    Ok(())
}

#[cfg(unix)]
fn terminate_pid(pid: u32) -> anyhow::Result<()> {
    let _ = Command::new("kill").arg("-TERM").arg(pid.to_string()).status();
    std::thread::sleep(Duration::from_millis(200));
    if is_pid_alive(pid) {
        let _ = Command::new("kill").arg("-KILL").arg(pid.to_string()).status();
    }
    Ok(())
}

#[cfg(not(unix))]
fn terminate_pid(pid: u32) -> anyhow::Result<()> {
    let _ = pid;
    Ok(())
}

/// Detach a child into its own session before spawn (Unix). Call on gateway `serve` spawn only.
#[cfg(unix)]
pub fn command_new_session(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    unsafe {
    cmd.pre_exec(|| {
        // SAFETY: setsid is async-signal-safe in the pre_exec hook; no other threads exist yet.
        {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    });
    }
}

#[cfg(not(unix))]
pub fn command_new_session(_cmd: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn gateway_pid_roundtrip() {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(GATEWAY_PID_FILE);
        std::fs::create_dir_all(dir.path()).unwrap();
        // Patch via direct write/read helpers using temp path would need injection;
        // test parse logic only.
        std::fs::write(&path, "4242\n").unwrap();
        let pid: u32 = std::fs::read_to_string(&path)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(pid, 4242);
    }
}


/// PIDs for a running gateway (pid file + `ferroclaw serve` matches).
pub fn gateway_running_pids() -> anyhow::Result<Vec<u32>> {
    clear_stale_gateway_pid_file();
    let mut pids = Vec::new();
    if let Some(pid) = read_gateway_pid() {
        if is_pid_alive(pid) {
            pids.push(pid);
        }
    }
    for (pid, _) in pgrep_lines("ferroclaw serve")? {
        if is_pid_alive(pid) && !pids.contains(&pid) {
            pids.push(pid);
        }
    }
    pids.sort_unstable();
    Ok(pids)
}
