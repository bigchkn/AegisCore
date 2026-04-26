use aegis_core::{
    AegisError, AegisEvent, Channel, ChannelKind, DetectedEvent, Message as CoreMessage, Result,
};
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use tokio::sync::mpsc;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "AegisCore commands:")]
pub enum Command {
    #[command(description = "display this text.")]
    Help,
    #[command(description = "show status summary.")]
    Status,
    #[command(description = "list active agents.")]
    Agents,
    #[command(description = "start the bot and show chat ID.")]
    Start,
    #[command(description = "pause an agent: /pause <uuid>")]
    Pause(String),
    #[command(description = "resume an agent: /resume <uuid>")]
    Resume(String),
    #[command(description = "kill an agent: /kill <uuid>")]
    Kill(String),
    #[command(
        description = "spawn a splinter: /spawn <role> <task>",
        parse_with = "split"
    )]
    Spawn { role: String, task: String },
    #[command(description = "show logs: /logs <uuid> [n]", parse_with = "split")]
    Logs { agent_id: String, lines: String },
    #[command(description = "manually trigger failover: /failover <uuid>")]
    Failover(String),
}

pub struct TelegramConfig {
    pub token: String,
    pub allowed_chat_ids: Vec<i64>,
}

pub struct TelegramBridge {
    bot: Bot,
    config: Arc<TelegramConfig>,
}

impl TelegramBridge {
    pub fn new(config: TelegramConfig) -> Self {
        let bot = Bot::new(config.token.clone());
        Self {
            bot,
            config: Arc::new(config),
        }
    }

    pub fn as_channel(&self, name: String) -> TelegramChannel {
        TelegramChannel {
            name,
            bot: self.bot.clone(),
            config: self.config.clone(),
        }
    }

    /// Run the bot loop and the event publisher task.
    pub async fn run(&self, mut event_rx: mpsc::Receiver<AegisEvent>) -> Result<()> {
        let bot = self.bot.clone();
        let config = self.config.clone();

        // 1. Start Event Publisher Task
        let pub_bot = bot.clone();
        let pub_config = config.clone();
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                let message = format_event(&event);
                for chat_id in &pub_config.allowed_chat_ids {
                    let _ = pub_bot.send_message(ChatId(*chat_id), &message).await;
                }
            }
        });

        // 2. Start Command Dispatcher
        let handler = dptree::entry().branch(
            Update::filter_message()
                .filter(move |msg: Message, cfg: Arc<TelegramConfig>| {
                    cfg.allowed_chat_ids.contains(&msg.chat.id.0)
                })
                .filter_command::<Command>()
                .endpoint(handle_command),
        );

        Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![config])
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;

        Ok(())
    }
}

pub struct TelegramChannel {
    name: String,
    bot: Bot,
    config: Arc<TelegramConfig>,
}

#[async_trait::async_trait]
impl Channel for TelegramChannel {
    fn kind(&self) -> ChannelKind {
        ChannelKind::Telegram
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_active(&self) -> bool {
        true
    }

    async fn send(&self, message: &CoreMessage) -> Result<()> {
        let text = format!(
            "📢 *Channel Message*\nFrom: `{:?}`\nType: `{:?}`\nPayload: ```json\n{}\n```",
            message.from, message.kind, message.payload
        );
        for chat_id in &self.config.allowed_chat_ids {
            self.bot
                .send_message(ChatId(*chat_id), &text)
                .await
                .map_err(|e| AegisError::Config {
                    field: "telegram".into(),
                    reason: e.to_string(),
                })?;
        }
        Ok(())
    }
}

async fn handle_command(bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    match cmd {
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?;
        }
        Command::Start => {
            bot.send_message(
                msg.chat.id,
                format!("AegisCore Bot active.\nChat ID: `{}`", msg.chat.id),
            )
            .await?;
        }
        Command::Status => {
            bot.send_message(msg.chat.id, "System status: Operational (Mocked)")
                .await?;
        }
        Command::Agents => {
            bot.send_message(msg.chat.id, "No active agents (Mocked)")
                .await?;
        }
        Command::Pause(id) => {
            bot.send_message(msg.chat.id, format!("Pausing agent {} (Mocked)", id))
                .await?;
        }
        Command::Resume(id) => {
            bot.send_message(msg.chat.id, format!("Resuming agent {} (Mocked)", id))
                .await?;
        }
        Command::Kill(id) => {
            bot.send_message(msg.chat.id, format!("Killing agent {} (Mocked)", id))
                .await?;
        }
        Command::Spawn { role, task } => {
            bot.send_message(
                msg.chat.id,
                format!("Spawning {} for: {} (Mocked)", role, task),
            )
            .await?;
        }
        Command::Logs { agent_id, lines } => {
            let n = lines.parse::<usize>().unwrap_or(20);
            bot.send_message(
                msg.chat.id,
                format!("Fetching last {} logs for {} (Mocked)", n, agent_id),
            )
            .await?;
        }
        Command::Failover(id) => {
            bot.send_message(
                msg.chat.id,
                format!("Triggering failover for {} (Mocked)", id),
            )
            .await?;
        }
    }
    Ok(())
}

fn format_event(event: &AegisEvent) -> String {
    match event {
        AegisEvent::AgentSpawned { agent_id, role } => {
            format!("🚀 *Agent Spawned*\nID: `{}`\nRole: `{}`", agent_id, role)
        }
        AegisEvent::AgentStatusChanged {
            agent_id,
            old_status,
            new_status,
        } => {
            format!(
                "🔄 *Status Changed*\nAgent: `{}`\n`{:?}` ➔ `{:?}`",
                agent_id, old_status, new_status
            )
        }
        AegisEvent::TaskComplete {
            task_id,
            receipt_path,
        } => {
            format!(
                "✅ *Task Complete*\nID: `{}`\nReceipt: `{}`",
                task_id, receipt_path
            )
        }
        AegisEvent::WatchdogAlert { event, action } => {
            let detail = match event {
                DetectedEvent::RateLimit {
                    agent_id,
                    matched_pattern,
                } => {
                    format!(
                        "Rate limit on `{}` (matched: `{}`)",
                        agent_id, matched_pattern
                    )
                }
                DetectedEvent::AuthFailure {
                    agent_id,
                    matched_pattern,
                } => {
                    format!(
                        "Auth failure on `{}` (matched: `{}`)",
                        agent_id, matched_pattern
                    )
                }
                DetectedEvent::CliCrash {
                    agent_id,
                    exit_code,
                } => {
                    format!("CLI crash on `{}` (code: {:?})", agent_id, exit_code)
                }
                DetectedEvent::SandboxViolation {
                    agent_id,
                    matched_pattern,
                } => {
                    format!(
                        "Sandbox violation on `{}` (matched: `{}`)",
                        agent_id, matched_pattern
                    )
                }
                DetectedEvent::TaskComplete {
                    agent_id,
                    matched_pattern,
                } => {
                    format!(
                        "Watchdog detected completion on `{}` (matched: `{}`)",
                        agent_id, matched_pattern
                    )
                }
            };
            format!("⚠️ *Watchdog Alert*\n{}\nAction: `{:?}`", detail, action)
        }
        AegisEvent::SystemNotification { message } => {
            format!("ℹ️ *System*\n{}", message)
        }
        _ => "ℹ️ *System*\nUnhandled event".to_string(),
    }
}

/// Simple variable replacement for telegram messages
pub fn render_message(template: &str, vars: &[(&str, &str)]) -> String {
    let mut rendered = template.to_string();
    for (key, value) in vars {
        let pattern = format!("{{{{{}}}}}", key);
        rendered = rendered.replace(&pattern, value);
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_render_message() {
        let template = "🚀 Agent {{id}} spawned in {{role}}";
        let vars = [("id", "123"), ("role", "architect")];
        assert_eq!(
            render_message(template, &vars),
            "🚀 Agent 123 spawned in architect"
        );
    }

    #[test]
    fn test_format_event() {
        let id = Uuid::nil();
        let event = AegisEvent::AgentSpawned {
            agent_id: id,
            role: "architect".into(),
        };
        let msg = format_event(&event);
        assert!(msg.contains("Agent Spawned"));
        assert!(msg.contains("architect"));
    }
}
