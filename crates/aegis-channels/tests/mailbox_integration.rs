use aegis_channels::mailbox::MailboxChannel;
use aegis_core::{Channel, Message, MessageSource, MessageType, Result};
use tempfile::tempdir;
use uuid::Uuid;

#[tokio::test]
async fn test_mailbox_send() -> Result<()> {
    let dir = tempdir().map_err(|e| aegis_core::error::AegisError::Unexpected(Box::new(e)))?;
    let channel = MailboxChannel::new("test-mailbox".to_string(), dir.path().to_path_buf());

    let message = Message::new(
        MessageSource::System,
        Uuid::new_v4(),
        MessageType::Notification,
        serde_json::json!({"test": "data"}),
    );

    channel.send(&message).await?;

    let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(entries.len(), 1);

    let entry = entries[0].as_ref().unwrap();
    let content = std::fs::read_to_string(entry.path()).unwrap();
    let read_message: Message = serde_json::from_str(&content).unwrap();

    assert_eq!(read_message.message_id, message.message_id);
    assert_eq!(read_message.payload["test"], "data");

    Ok(())
}
