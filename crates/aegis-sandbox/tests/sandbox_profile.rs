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
#[cfg(target_os = "macos")]
fn file_access_denied_outside_worktree() {
    if Command::new("which")
        .arg("sandbox-exec")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| !status.success())
        .unwrap_or(true)
    {
        return;
    }

    let dir = temp_path("exec");
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

    let status = Command::new("sandbox-exec")
        .arg("-f")
        .arg(&profile)
        .arg("/bin/cat")
        .arg("/etc/passwd")
        .status()
        .expect("sandbox-exec launches");

    assert!(!status.success());
    let _ = fs::remove_dir_all(dir);
}
