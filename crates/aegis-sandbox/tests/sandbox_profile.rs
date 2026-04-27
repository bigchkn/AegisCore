use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use aegis_core::{SandboxNetworkPolicy, SandboxPolicy, SandboxProfile};
use aegis_sandbox::{SandboxError, SeatbeltSandbox};

fn sandbox() -> SeatbeltSandbox {
    SeatbeltSandbox::new()
}

fn temp_path(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time moves forward")
        .as_nanos();
    std::env::temp_dir().join(format!("aegis-sandbox-{name}-{suffix}"))
}

/// Returns a path under /private/tmp (canonical, no symlinks) for use in exec tests.
/// Seatbelt resolves symlinks when checking process-exec rules, so paths through /var/folders
/// (which is a symlink to /private/var/folders) will not match non-canonical SBPL rules.
#[cfg(target_os = "macos")]
fn canonical_temp_path(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time moves forward")
        .as_nanos();
    PathBuf::from(format!("/private/tmp/aegis-sandbox-{name}-{suffix}"))
}

fn sandbox_exec_available() -> bool {
    Command::new("which")
        .arg("sandbox-exec")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
fn render_contains_worktree() {
    let rendered = sandbox()
        .render(
            Path::new("/tmp/aegis-worktree"),
            Path::new("/Users/tester"),
            &SandboxPolicy::default(),
        )
        .expect("profile renders");

    assert!(rendered.contains("(subpath \"/tmp/aegis-worktree\")"));
    assert!(!rendered.contains("@@"));
}

#[test]
fn render_outbound_only() {
    let rendered = sandbox()
        .render(
            Path::new("/tmp/aegis-worktree"),
            Path::new("/Users/tester"),
            &SandboxPolicy::default(),
        )
        .expect("profile renders");

    assert!(rendered.contains("(allow network-outbound)"));
    assert!(rendered.contains("(deny network-inbound)"));
}

#[test]
fn render_no_network() {
    let policy = SandboxPolicy {
        network: SandboxNetworkPolicy::None,
        ..SandboxPolicy::default()
    };

    let rendered = sandbox()
        .render(
            Path::new("/tmp/aegis-worktree"),
            Path::new("/Users/tester"),
            &policy,
        )
        .expect("profile renders");

    assert!(rendered.contains("(deny network*)"));
    assert!(!rendered.contains("(allow network-outbound)"));
}

#[test]
fn render_extra_reads_and_writes() {
    let policy = SandboxPolicy {
        extra_reads: vec![PathBuf::from("/usr/local/share/zsh")],
        extra_writes: vec![PathBuf::from("/tmp/aegis-cache")],
        ..SandboxPolicy::default()
    };

    let rendered = sandbox()
        .render(
            Path::new("/tmp/aegis-worktree"),
            Path::new("/Users/tester"),
            &policy,
        )
        .expect("profile renders");

    assert!(rendered.contains("(allow file-read*\n  (subpath \"/usr/local/share/zsh\"))"));
    assert!(rendered.contains("(allow file-write*\n  (subpath \"/tmp/aegis-cache\"))"));
}

#[test]
fn hard_deny_ssh_is_always_present() {
    let policy = SandboxPolicy {
        extra_reads: vec![PathBuf::from("/Users/tester")],
        ..SandboxPolicy::default()
    };

    let rendered = sandbox()
        .render(
            Path::new("/tmp/aegis-worktree"),
            Path::new("/Users/tester"),
            &policy,
        )
        .expect("profile renders");

    assert!(rendered.contains("(subpath \"/Users/tester/.ssh\")"));
    assert!(rendered.contains("(subpath \"/tmp/aegis-worktree/.aegis/logs/sessions\")"));
}

#[test]
fn render_uses_configured_logs_dir_when_available() {
    let rendered =
        SeatbeltSandbox::with_logs_dir(PathBuf::from("/tmp/project/.aegis/logs/sessions"))
            .render(
                Path::new("/tmp/aegis-worktree"),
                Path::new("/Users/tester"),
                &SandboxPolicy::default(),
            )
            .expect("profile renders");

    assert!(rendered.contains("(subpath \"/tmp/project/.aegis/logs/sessions\")"));
    assert!(!rendered.contains("(subpath \"/tmp/aegis-worktree/.aegis/logs/sessions\")"));
}

#[test]
fn render_policy_hard_deny_reads() {
    let policy = SandboxPolicy {
        hard_deny_reads: vec![PathBuf::from("/tmp/aegis-worktree/secrets")],
        ..SandboxPolicy::default()
    };

    let rendered = sandbox()
        .render(
            Path::new("/tmp/aegis-worktree"),
            Path::new("/Users/tester"),
            &policy,
        )
        .expect("profile renders");

    assert!(rendered.contains("(deny file-read*\n  (subpath \"/tmp/aegis-worktree/secrets\"))"));
}

#[test]
fn write_atomic_creates_profile_and_removes_tmp() {
    let dir = temp_path("write");
    let dest = dir.join("agent.sb");

    sandbox()
        .write(
            Path::new("/tmp/aegis-worktree"),
            Path::new("/Users/tester"),
            &SandboxPolicy::default(),
            &dest,
        )
        .expect("profile writes");

    let content = fs::read_to_string(&dest).expect("profile exists");
    assert!(content.contains("(deny default)"));
    assert!(!dest.with_extension("sb.tmp").exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&dest).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn exec_prefix() {
    let prefix = sandbox().exec_prefix(Path::new("/tmp/profile.sb"));
    assert_eq!(prefix, vec!["sandbox-exec", "-f", "/tmp/profile.sb"]);
}

#[test]
fn sandbox_error_maps_to_core_error() {
    let error: aegis_core::AegisError = SandboxError::SandboxExecNotFound.into();
    assert!(matches!(
        error,
        aegis_core::AegisError::SandboxProfileRender { .. }
    ));
}

#[test]
fn render_includes_extra_exec_paths_in_process_exec() {
    let policy = SandboxPolicy {
        extra_exec_paths: vec![PathBuf::from("/custom/bin")],
        ..SandboxPolicy::default()
    };

    let rendered = sandbox()
        .render(
            Path::new("/tmp/aegis-worktree"),
            Path::new("/Users/tester"),
            &policy,
        )
        .expect("profile renders");

    assert!(rendered.contains("(allow process-exec\n  (subpath \"/custom/bin\"))"));
    assert!(rendered.contains("(allow process-exec\n  (subpath \"/Users/tester/.local\"))"));
    assert!(!rendered.contains("@@"));
}

/// Demonstrates the bug: a script at a path outside the hardcoded process-exec rules
/// cannot be executed under the default sandbox policy.
///
/// macOS exec syscall checks the script's own path against process-exec rules (shebang causes
/// the kernel to exec the interpreter, but the script path is still checked first). So a shell
/// script in a custom dir is a reliable, dependency-free way to test process-exec enforcement.
#[test]
#[cfg(target_os = "macos")]
fn exec_path_outside_policy_is_denied() {
    if !sandbox_exec_available() {
        return;
    }

    let wt = canonical_temp_path("deny-wt");
    let bin_dir = canonical_temp_path("deny-bins");
    fs::create_dir_all(&wt).expect("worktree dir");
    fs::create_dir_all(&bin_dir).expect("bin dir");

    // Shell script: the script path itself is checked for process-exec on macOS (shebang exec).
    let test_bin = bin_dir.join("run.sh");
    fs::write(&test_bin, "#!/bin/sh\nexit 0\n").expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&test_bin, fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    let profile = wt.join("agent.sb");
    sandbox()
        .write(
            &wt,
            Path::new("/Users/tester"),
            &SandboxPolicy {
                network: SandboxNetworkPolicy::None,
                ..SandboxPolicy::default()
            },
            &profile,
        )
        .expect("profile writes");

    let status = Command::new("sandbox-exec")
        .arg("-f")
        .arg(&profile)
        .arg(&test_bin)
        .status()
        .expect("sandbox-exec launches");

    assert!(
        !status.success(),
        "script at {:?} should be exec-denied by default policy",
        test_bin
    );

    let _ = fs::remove_dir_all(&wt);
    let _ = fs::remove_dir_all(&bin_dir);
}

/// Validates the fix: the same script executes when its parent dir is in extra_exec_paths.
#[test]
#[cfg(target_os = "macos")]
fn exec_path_via_extra_exec_paths_is_allowed() {
    if !sandbox_exec_available() {
        return;
    }

    let wt = canonical_temp_path("allow-wt");
    let bin_dir = canonical_temp_path("allow-bins");
    fs::create_dir_all(&wt).expect("worktree dir");
    fs::create_dir_all(&bin_dir).expect("bin dir");

    let test_bin = bin_dir.join("run.sh");
    fs::write(&test_bin, "#!/bin/sh\nexit 0\n").expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&test_bin, fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    let profile = wt.join("agent.sb");
    sandbox()
        .write(
            &wt,
            Path::new("/Users/tester"),
            &SandboxPolicy {
                network: SandboxNetworkPolicy::None,
                extra_exec_paths: vec![bin_dir.clone()],
                ..SandboxPolicy::default()
            },
            &profile,
        )
        .expect("profile writes");

    let status = Command::new("sandbox-exec")
        .arg("-f")
        .arg(&profile)
        .arg(&test_bin)
        .status()
        .expect("sandbox-exec launches");

    assert!(
        status.success(),
        "script at {:?} should execute when its dir is in extra_exec_paths",
        test_bin
    );

    let _ = fs::remove_dir_all(&wt);
    let _ = fs::remove_dir_all(&bin_dir);
}

/// With the broad read policy, agents CAN read any file (except hard-denied credential paths).
/// What they cannot do is WRITE outside the worktree and temp space.
/// This test verifies that write attempts to non-temp system paths are denied.
#[test]
#[cfg(target_os = "macos")]
fn write_denied_outside_worktree() {
    if !sandbox_exec_available() {
        return;
    }

    let dir = canonical_temp_path("write-wt");
    fs::create_dir_all(&dir).expect("temp dir");
    let profile = dir.join("agent.sb");

    sandbox()
        .write(
            &dir,
            Path::new("/Users/tester"),
            &SandboxPolicy {
                network: SandboxNetworkPolicy::None,
                ..SandboxPolicy::default()
            },
            &profile,
        )
        .expect("profile writes");

    // Attempt to create a file in /usr/bin — allowed for exec but not for writes.
    let status = Command::new("sandbox-exec")
        .arg("-f")
        .arg(&profile)
        .arg("/usr/bin/touch")
        .arg("/usr/bin/aegis_sandbox_test_canary")
        .status()
        .expect("sandbox-exec launches");

    assert!(!status.success(), "write to /usr/bin should be denied");
    let _ = fs::remove_dir_all(dir);
}
