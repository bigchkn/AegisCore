pub mod manifest;
pub mod registry;
pub mod handoff;

#[cfg(feature = "claude")]
pub mod claude;
#[cfg(feature = "gemini")]
pub mod gemini;
#[cfg(feature = "codex")]
pub mod codex;
#[cfg(feature = "ollama")]
pub mod ollama;

pub use registry::ProviderRegistry;
