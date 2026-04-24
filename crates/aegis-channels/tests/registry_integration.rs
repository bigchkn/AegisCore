use aegis_core::{ChannelKind, ChannelRegistry, Result};
use aegis_channels::registry::FileChannelRegistry;
use tempfile::tempdir;

#[test]
fn test_registry_lifecycle() -> Result<()> {
    let dir = tempdir().map_err(|e| aegis_core::error::AegisError::Unexpected(Box::new(e)))?;
    let path = dir.path().join("channels.json");
    let registry = FileChannelRegistry::new(path);

    // Initial state
    let list = registry.list()?;
    assert!(list.is_empty());

    // Register
    registry.register("telegram", ChannelKind::Telegram)?;
    let channel = registry.get("telegram")?.unwrap();
    assert_eq!(channel.name, "telegram");
    assert_eq!(channel.kind, ChannelKind::Telegram);

    // List
    let list = registry.list()?;
    assert_eq!(list.len(), 1);

    // Deregister
    registry.deregister("telegram")?;
    assert!(registry.get("telegram")?.is_none());
    assert!(registry.list()?.is_empty());

    Ok(())
}
