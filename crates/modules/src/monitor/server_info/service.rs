//! Server info service — reads system metrics.

use super::dto::ServerInfoDto;
use framework::error::AppError;

/// Gather server system information.
#[tracing::instrument(skip_all)]
pub async fn get_server_info() -> Result<ServerInfoDto, AppError> {
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    // Try to read /proc/meminfo on Linux; fallback to zeros on other platforms
    let (memory_total_mb, memory_used_mb, memory_usage_percent) = read_memory_info();

    // Uptime: read /proc/uptime on Linux; fallback to 0
    let uptime_secs = read_uptime();

    let rust_version = env!("CARGO_PKG_RUST_VERSION", "unknown").to_string();
    let os_info = format!("{} {}", std::env::consts::OS, std::env::consts::FAMILY);
    let arch = std::env::consts::ARCH.to_string();

    Ok(ServerInfoDto {
        cpu_cores,
        memory_total_mb,
        memory_used_mb,
        memory_usage_percent,
        uptime_secs,
        rust_version,
        os_info,
        arch,
    })
}

fn read_memory_info() -> (u64, u64, f64) {
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            let mut total_kb: u64 = 0;
            let mut available_kb: u64 = 0;
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    total_kb = parse_meminfo_value(line);
                } else if line.starts_with("MemAvailable:") {
                    available_kb = parse_meminfo_value(line);
                }
            }
            let total_mb = total_kb / 1024;
            let used_mb = total_mb.saturating_sub(available_kb / 1024);
            let usage = if total_mb > 0 {
                (used_mb as f64 / total_mb as f64) * 100.0
            } else {
                0.0
            };
            return (total_mb, used_mb, (usage * 100.0).round() / 100.0);
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Use sysctl on macOS
        if let Ok(output) = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
        {
            if let Ok(s) = std::str::from_utf8(&output.stdout) {
                if let Ok(bytes) = s.trim().parse::<u64>() {
                    let total_mb = bytes / (1024 * 1024);
                    // Approximate usage via vm_stat is complex; return total only
                    return (total_mb, 0, 0.0);
                }
            }
        }
    }

    (0, 0, 0.0)
}

#[cfg(target_os = "linux")]
fn parse_meminfo_value(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn read_uptime() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/uptime") {
            if let Some(first) = content.split_whitespace().next() {
                if let Ok(secs) = first.parse::<f64>() {
                    return secs as u64;
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("sysctl")
            .args(["-n", "kern.boottime"])
            .output()
        {
            if let Ok(s) = std::str::from_utf8(&output.stdout) {
                // kern.boottime returns something like "{ sec = 1712345678, usec = 123456 }"
                if let Some(sec_start) = s.find("sec = ") {
                    let after = &s[sec_start + 6..];
                    if let Some(end) = after.find(',') {
                        if let Ok(boot_sec) = after[..end].trim().parse::<i64>() {
                            let now = chrono::Utc::now().timestamp();
                            return (now - boot_sec).max(0) as u64;
                        }
                    }
                }
            }
        }
    }

    0
}
