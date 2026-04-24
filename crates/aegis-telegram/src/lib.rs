use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use std::sync::Arc;
use aegis_core::{Result, AegisError};

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

    pub async fn run(&self) -> Result<()> {
        let bot = self.bot.clone();
        let config = self.config.clone();

        let handler = dptree::entry()
            .branch(
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

async fn handle_command(bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    match cmd {
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string()).await?;
        }
        Command::Start => {
            bot.send_message(
                msg.chat.id,
                format!("AegisCore Bot active.\nChat ID: `{}`", msg.chat.id),
            )
            .await?;
        }
        Command::Status => {
            bot.send_message(msg.chat.id, "System status: Operational (Mocked)").await?;
        }
        Command::Agents => {
            bot.send_message(msg.chat.id, "No active agents (Mocked)").await?;
        }
    }
    Ok(())
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

    #[test]
    fn test_render_message() {
        let template = "🚀 Agent {{id}} spawned in {{role}}";
        let vars = [("id", "123"), ("role", "architect")];
        assert_eq!(render_message(template, &vars), "🚀 Agent 123 spawned in architect");
    }
}
