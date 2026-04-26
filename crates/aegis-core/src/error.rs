use std::{io, path::PathBuf};
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, AegisError>;

#[derive(Debug, thiserror::Error)]
pub enum AegisError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("config error — field `{field}`: {reason}")]
    Config { field: String, reason: String },

    // ── Config ───────────────────────────────────────────────────────
    #[error("config file not found: {path}")]
    ConfigNotFound { path: PathBuf },

    #[error("config parse error at {path}: {source}")]
    ConfigParseError {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("config serialization error at {path}: {source}")]
    ConfigSerializationError {
        path: PathBuf,
        #[source]
        source: toml::ser::Error,
    },

    #[error("config validation error — field `{field}`: {reason}")]
    ConfigValidation { field: String, reason: String },

    // ── Registry ─────────────────────────────────────────────────────
    #[error("registry lock error: {source}")]
    RegistryLock {
        #[source]
        source: io::Error,
    },

    #[error("registry file corrupted at {path}: {source}")]
    RegistryCorrupted {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("agent not found: {agent_id}")]
    AgentNotFound { agent_id: Uuid },

    #[error("task not found: {task_id}")]
    TaskNotFound { task_id: Uuid },

    #[error("receipt not found for task {task_id} at {path}")]
    ReceiptNotFound { task_id: Uuid, path: PathBuf },

    #[error("invalid receipt at {path}: {reason}")]
    ReceiptInvalid { path: PathBuf, reason: String },

    // ── Tmux ─────────────────────────────────────────────────────────
    #[error("tmux command `{command}` failed: {stderr}")]
    TmuxCommand { command: String, stderr: String },

    #[error("tmux session not found: {target}")]
    TmuxSessionNotFound { target: String },

    #[error("tmux pane not found: {target}")]
    TmuxPaneNotFound { target: String },

    // ── Sandbox ──────────────────────────────────────────────────────
    #[error("sandbox profile render failed: {reason}")]
    SandboxProfileRender { reason: String },

    #[error("sandbox-exec failed to start: {source}")]
    SandboxExec {
        #[source]
        source: io::Error,
    },

    // ── Provider ─────────────────────────────────────────────────────
    #[error("provider not found: `{name}`")]
    ProviderNotFound { name: String },

    #[error("provider `{provider}` failed to spawn: {source}")]
    ProviderSpawn {
        provider: String,
        #[source]
        source: io::Error,
    },

    // ── Channel ──────────────────────────────────────────────────────
    #[error("channel not found: `{name}`")]
    ChannelNotFound { name: String },

    #[error("channel send failed on `{kind}`: {reason}")]
    ChannelSend { kind: String, reason: String },

    // ── Recorder ─────────────────────────────────────────────────────
    #[error("recorder attach failed for agent {agent_id}: {source}")]
    RecorderAttach {
        agent_id: Uuid,
        #[source]
        source: io::Error,
    },

    #[error("log file not found for agent {agent_id} at {path}")]
    LogFileNotFound { agent_id: Uuid, path: PathBuf },

    // ── IPC / Daemon ─────────────────────────────────────────────────
    #[error("aegisd is not running (socket: {socket_path})")]
    DaemonNotRunning { socket_path: PathBuf },

    #[error("IPC connection failed: {source}")]
    IpcConnection {
        #[source]
        source: io::Error,
    },

    #[error("IPC stream closed unexpectedly")]
    IpcStreamClosed,

    #[error("IPC protocol error: {reason}")]
    IpcProtocol { reason: String },

    // ── Storage ──────────────────────────────────────────────────────
    #[error("storage I/O error at {path}: {source}")]
    StorageIo {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("not an AegisCore project (no .aegis/ found from {path})")]
    ProjectNotInitialized { path: PathBuf },

    #[error("project already initialized at {path}")]
    ProjectAlreadyInitialized { path: PathBuf },

    // ── Git ──────────────────────────────────────────────────────────
    #[error("git worktree add failed at {path}: {reason}")]
    GitWorktreeAdd { path: PathBuf, reason: String },

    #[error("git worktree prune failed: {reason}")]
    GitWorktreePrune { reason: String },

    #[error("git merge conflict on branch {branch}: {reason}")]
    GitMergeConflict { branch: String, reason: String },

    // ── General ──────────────────────────────────────────────────────
    #[error(transparent)]
    Unexpected(Box<dyn std::error::Error + Send + Sync>),
}
