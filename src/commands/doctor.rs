use std::path::PathBuf;
use crate::{anchoring::ProjectAnchor, client::DaemonClient, error::AegisCliError, output::Printer};

struct Check {
    label: String,
    ok: bool,
    detail: String,
}

pub async fn run(printer: &Printer, client: &DaemonClient) -> i32 {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let config = load_effective_config(&cwd);

    let mut checks: Vec<Check> = Vec::new();

    // 1. tmux
    checks.push(check_tmux());

    // 2. git
    checks.push(check_binary("git", &["--version"], "git"));

    // 3. node (optional but recommended for web UI development)
    checks.push(check_binary("node", &["--version"], "node"));

    // 4. sandbox-exec (macOS only)
    #[cfg(target_os = "macos")]
    checks.push(check_sandbox_exec());

    // 5. launchd (macOS only)
    #[cfg(target_os = "macos")]
    checks.push(check_launchd());

    // 6. aegisd
    checks.push(check_daemon(client).await);

    // 7. Directory permissions
    checks.push(check_aegis_dir());

    // 5. Configured providers
    if let Some(cfg) = &config {
        for (name, entry) in &cfg.providers {
            checks.push(check_provider(name, &entry.binary));
        }
    }

    println!("AegisCore Doctor");
    printer.separator();

    let mut failures = 0;
    for c in &checks {
        printer.status_line(c.ok, &c.label, &c.detail);
        if !c.ok {
            failures += 1;
        }
    }

    printer.separator();
    if failures == 0 {
        printer.line("All checks passed.");
        0
    } else {
        eprintln!("{failures} issue(s) found.");
        1
    }
}

fn check_tmux() -> Check {
    match std::process::Command::new("tmux").arg("-V").output() {
        Ok(out) if out.status.success() => {
            let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let ok = is_tmux_version_ok(&ver);
            Check {
                label: "tmux".into(),
                ok,
                detail: if ok { ver } else { format!("{ver} (need ≥ 3.0)") },
            }
        }
        _ => Check {
            label: "tmux".into(),
            ok: false,
            detail: "not found — install tmux ≥ 3.0".into(),
        },
    }
}

fn is_tmux_version_ok(ver_str: &str) -> bool {
    // "tmux 3.4" or "tmux 3.0a" → parse major version
    let parts: Vec<&str> = ver_str.split_whitespace().collect();
    if let Some(v) = parts.get(1) {
        // Handle cases like "3.0a" by taking only numeric prefix
        let major_str: String = v.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(major) = major_str.parse::<u32>() {
            return major >= 3;
        }
    }
    false
}

fn check_binary(label: &str, args: &[&str], _name: &str) -> Check {
    match std::process::Command::new(label).args(args).output() {
        Ok(out) if out.status.success() => {
            let ver = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            Check { label: label.into(), ok: true, detail: ver }
        }
        _ => Check {
            label: label.into(),
            ok: label == "node", // node is optional
            detail: if label == "node" { "optional - needed for web UI development".into() } else { format!("{label} not found") },
        },
    }
}

#[cfg(target_os = "macos")]
fn check_sandbox_exec() -> Check {
    let path = std::path::Path::new("/usr/bin/sandbox-exec");
    if path.exists() {
        Check { label: "sandbox-exec".into(), ok: true, detail: "present".into() }
    } else {
        Check {
            label: "sandbox-exec".into(),
            ok: false,
            detail: "not found at /usr/bin/sandbox-exec".into(),
        }
    }
}

#[cfg(target_os = "macos")]
fn check_launchd() -> Check {
    let home = std::env::var("HOME").unwrap_or_default();
    let plist_path = std::path::PathBuf::from(&home).join("Library/LaunchAgents/com.aegiscore.aegisd.plist");
    
    if !plist_path.exists() {
        return Check {
            label: "launchd plist".into(),
            ok: false,
            detail: "not installed - run: aegis daemon install".into(),
        };
    }

    // Check if it's loaded
    let output = std::process::Command::new("launchctl")
        .arg("list")
        .arg("com.aegiscore.aegisd")
        .output();

    match output {
        Ok(out) if out.status.success() => {
            Check {
                label: "launchd service".into(),
                ok: true,
                detail: "active".into(),
            }
        }
        _ => Check {
            label: "launchd service".into(),
            ok: false,
            detail: "installed but not loaded - run: aegis daemon start".into(),
        },
    }
}

fn check_aegis_dir() -> Check {
    let home = std::env::var("HOME").unwrap_or_default();
    let aegis_dir = std::path::PathBuf::from(&home).join(".aegis");

    if !aegis_dir.exists() {
        return Check {
            label: "~/.aegis".into(),
            ok: true,
            detail: "not present (will be created on first use)".into(),
        };
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&aegis_dir) {
            let mode = meta.permissions().mode() & 0o777;
            if mode != 0o700 {
                return Check {
                    label: "~/.aegis permissions".into(),
                    ok: false,
                    detail: format!("found {:o}, need 700", mode),
                };
            }
        }
    }

    Check {
        label: "~/.aegis".into(),
        ok: true,
        detail: "ok".into(),
    }
}

async fn check_daemon(client: &DaemonClient) -> Check {
    match client.request(None, "daemon.status", serde_json::json!({})).await {
        Ok(payload) => {
            let ver = payload
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let uptime = payload.get("uptime_s").and_then(|v| v.as_u64()).unwrap_or(0);
            Check {
                label: "aegisd".into(),
                ok: true,
                detail: format!("running (v{ver}, uptime {}s)", uptime),
            }
        }
        Err(AegisCliError::DaemonNotRunning) => Check {
            label: "aegisd".into(),
            ok: false,
            detail: "not running — run: aegis daemon start".into(),
        },
        Err(e) => Check {
            label: "aegisd".into(),
            ok: false,
            detail: format!("error: {e}"),
        },
    }
}

fn check_provider(name: &str, binary: &str) -> Check {
    let found = which_binary(binary);
    Check {
        label: format!("{name} provider"),
        ok: found.is_some(),
        detail: match found {
            Some(p) => p.display().to_string(),
            None => format!("{binary} not found"),
        },
    }
}

fn which_binary(name: &str) -> Option<PathBuf> {
    std::env::var("PATH").ok().and_then(|path_var| {
        path_var.split(':').find_map(|dir| {
            let p = PathBuf::from(dir).join(name);
            if p.is_file() { Some(p) } else { None }
        })
    })
}

fn load_effective_config(cwd: &std::path::Path) -> Option<aegis_core::config::EffectiveConfig> {
    use aegis_core::config::EffectiveConfig;
    let root = ProjectAnchor::discover(cwd)
        .map(|a| a.project_root)
        .unwrap_or_else(|_| cwd.to_path_buf());
    let global = EffectiveConfig::load_global().unwrap_or_default();
    let project = EffectiveConfig::load_project(&root).unwrap_or_default();
    EffectiveConfig::resolve(&global, &project).ok()
}
