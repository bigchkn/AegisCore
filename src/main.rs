mod anchoring;
mod client;
mod commands;
mod error;
mod output;

use anchoring::ProjectAnchor;
use clap::{Parser, Subcommand};
use clap_complete::Shell;
use client::DaemonClient;
use error::AegisCliError;
use output::Printer;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "aegis", about = "AegisCore CLI", version)]
struct Cli {
    /// Unix domain socket path
    #[arg(
        long,
        default_value = "/tmp/aegis.sock",
        global = true,
        env = "AEGIS_SOCKET"
    )]
    socket: PathBuf,

    /// Emit raw JSON output
    #[arg(long, global = true)]
    json: bool,

    /// Disable ANSI color output
    #[arg(long, global = true)]
    no_color: bool,

    /// Suppress informational messages
    #[arg(long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize an AegisCore project in the current directory
    Init {
        /// Reinitialize even if already initialized
        #[arg(long)]
        force: bool,
    },

    /// Check system dependencies
    Doctor,

    /// Daemon management
    Daemon {
        #[command(subcommand)]
        subcommand: DaemonCommands,
    },

    /// List all registered projects
    Projects,

    /// Start Bastion agents for this project
    Start {
        /// Start a specific Bastion role only
        #[arg(long)]
        bastion: Option<String>,
    },

    /// Stop all project agents
    Stop {
        /// Kill agents immediately (no graceful drain)
        #[arg(long)]
        force: bool,
    },

    /// Attach to the project tmux session (or a specific agent pane)
    Attach { agent_id: Option<Uuid> },

    /// List all agents
    Agents,

    /// Spawn a Splinter agent with a task
    Spawn {
        /// Task description
        task: String,
        /// Agent role
        #[arg(long)]
        role: Option<String>,
        /// Parent agent ID
        #[arg(long)]
        parent: Option<Uuid>,
    },

    /// Pause an agent
    Pause { agent_id: String },

    /// Resume a paused agent
    Resume { agent_id: String },

    /// Kill an agent
    Kill { agent_id: String },

    /// Trigger provider failover for an agent
    Failover { agent_id: String },

    /// Channel management
    Channel {
        #[command(subcommand)]
        subcommand: ChannelCommands,
    },

    /// Show project status overview
    Status,

    /// Start the interactive TUI
    Ui,

    /// Tail an agent's Flight Recorder log
    Logs {
        agent_id: String,
        /// Number of lines
        #[arg(short = 'n', default_value_t = 50)]
        lines: usize,
        /// Stream log continuously
        #[arg(long)]
        follow: bool,
    },

    /// Send or inspect agent-to-agent messages
    Message {
        #[command(subcommand)]
        subcommand: MessageCommands,
    },

    /// Request or answer human clarifications
    Clarify {
        #[command(subcommand)]
        subcommand: ClarifyCommands,
    },

    /// Config management
    Config {
        #[command(subcommand)]
        subcommand: ConfigCommands,
    },

    /// Taskflow pipeline management
    Taskflow {
        #[command(subcommand)]
        subcommand: TaskflowCommands,
    },

    /// Agent design and template management
    Design {
        #[command(subcommand)]
        subcommand: DesignCommands,
    },

    /// Generate shell completions
    Completions { shell: Shell },
}

#[derive(Subcommand)]
enum DesignCommands {
    /// List all available templates
    List,
    /// Show details for a template
    Show { name: String },
    /// Spawn an agent from a template
    Spawn {
        name: String,
        /// Override the agent's model
        #[arg(long)]
        model: Option<String>,
        /// Set a template variable: KEY=VALUE
        #[arg(long = "var", value_name = "KEY=VALUE")]
        vars: Vec<String>,
    },
    /// Write a template as an [agent.<role>] block in aegis.toml
    Apply {
        name: String,
        /// Override the agent role name in the output TOML
        #[arg(long)]
        role: Option<String>,
        /// Set a template variable: KEY=VALUE
        #[arg(long = "var", value_name = "KEY=VALUE")]
        vars: Vec<String>,
    },
    /// Scaffold a blank project-local template
    New {
        name: String,
        /// Template kind: bastion or splinter
        #[arg(long, default_value = "bastion")]
        kind: String,
    },
}

#[derive(Subcommand)]
enum DaemonCommands {
    /// Start the daemon via launchd
    Start,
    /// Stop the daemon via launchd
    Stop,
    /// Show daemon status
    Status,
    /// Install the launchd plist
    Install,
    /// Uninstall the launchd plist
    Uninstall,
}

#[derive(Subcommand)]
enum ChannelCommands {
    /// Add a channel
    Add {
        #[command(subcommand)]
        kind: ChannelAddKind,
    },
    /// List active channels
    List,
    /// Show channel health and stats
    Status { name: String },
    /// Remove a channel
    Remove {
        name: String,
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum ChannelAddKind {
    /// Add Telegram bridge
    Telegram {
        #[arg(long)]
        token: Option<String>,
        #[arg(long = "chat-id")]
        chat_ids: Vec<i64>,
        #[arg(long)]
        yes: bool,
    },
    /// Add a named mailbox channel
    Mailbox { name: String },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Validate aegis.toml
    Validate,
    /// Show effective merged config
    Show,
}

#[derive(Subcommand)]
enum TaskflowCommands {
    /// Show project pipeline overview
    Status,
    /// List all milestones
    List,
    /// Show detailed tasks for a milestone
    Show { milestone_id: String },
    /// Sync roadmap with agent registry
    Sync,
    /// Manually link a roadmap task to a registry task
    Assign { roadmap_id: String, task_id: String },
    /// Create a new milestone
    CreateMilestone {
        id: String,
        name: String,
        /// Optional path to LLD document
        #[arg(long)]
        lld: Option<String>,
    },
    /// Add a task to a milestone
    AddTask {
        /// Short unique ID (e.g., 13.1)
        id: String,
        /// Task description
        task: String,
        /// Milestone ID (defaults to 'backlog' if omitted)
        milestone_id: Option<String>,
        /// Mark as a bug
        #[arg(long)]
        bug: bool,
        /// Mark as maintenance
        #[arg(long)]
        maint: bool,
    },
    /// Update the status of a roadmap task
    SetTaskStatus {
        milestone_id: String,
        task_id: String,
        status: String,
    },
    /// Show the next milestone to work on (greedy topological order)
    Next,
}

#[derive(Subcommand)]
enum MessageCommands {
    /// Send a message to an agent inbox
    Send {
        to_agent_id: String,
        message: String,
        #[arg(long)]
        from_agent_id: Option<Uuid>,
        #[arg(long, value_enum, default_value_t = commands::messages::MessageKindArg::Notification)]
        kind: commands::messages::MessageKindArg,
    },
    /// Inspect one agent inbox
    Inbox { agent_id: String },
    /// List inbox summaries or a specific inbox when an agent ID is supplied
    List { agent_id: Option<String> },
}

#[derive(Subcommand)]
enum ClarifyCommands {
    /// Create a clarification request for an agent
    Request {
        agent_id: String,
        question: String,
        #[arg(long)]
        task_id: Option<String>,
        #[arg(long, default_value_t = 0)]
        priority: i32,
        #[arg(long)]
        context: Option<String>,
        #[arg(long)]
        wait: bool,
    },
    /// List clarification requests
    List {
        #[arg(long)]
        agent_id: Option<String>,
    },
    /// Show one clarification request
    Show { request_id: String },
    /// Answer one clarification request
    Answer {
        request_id: String,
        answer: String,
        #[arg(long)]
        payload: Option<String>,
    },
    /// Block until a clarification resolves
    Wait {
        request_or_agent_id: String,
        #[arg(long)]
        timeout_secs: Option<u64>,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let printer = Printer::new(cli.json, cli.no_color);
    let client = DaemonClient::new(cli.socket.clone());

    let result = dispatch(cli, &printer, &client).await;

    if let Err(e) = result {
        e.print_and_exit();
    }
}

async fn dispatch(cli: Cli, printer: &Printer, client: &DaemonClient) -> Result<(), AegisCliError> {
    match cli.command {
        Commands::Init { force } => commands::init::run(force, printer, client).await,

        Commands::Doctor => {
            let code = commands::doctor::run(printer, client).await;
            if code != 0 {
                std::process::exit(code);
            }
            Ok(())
        }

        Commands::Daemon { subcommand } => match subcommand {
            DaemonCommands::Start => commands::daemon::start(printer).await,
            DaemonCommands::Stop => commands::daemon::stop(printer).await,
            DaemonCommands::Status => commands::daemon::status(printer, client).await,
            DaemonCommands::Install => commands::daemon::install().await,
            DaemonCommands::Uninstall => commands::daemon::uninstall().await,
        },

        Commands::Projects => commands::daemon::projects(printer, client).await,

        Commands::Start { bastion } => {
            let anchor = require_anchor()?;
            commands::session::start(bastion.as_deref(), printer, client, &anchor).await
        }

        Commands::Stop { force } => {
            let anchor = require_anchor()?;
            commands::session::stop(force, printer, client, &anchor).await
        }

        Commands::Attach { agent_id } => {
            let anchor = require_anchor()?;
            commands::session::attach(agent_id, &anchor)
        }

        Commands::Agents => {
            let anchor = require_anchor()?;
            commands::agents::list(printer, client, &anchor).await
        }

        Commands::Spawn { task, role, parent } => {
            let anchor = require_anchor()?;
            commands::agents::spawn(&task, role.as_deref(), parent, printer, client, &anchor).await
        }

        Commands::Pause { agent_id } => {
            let anchor = require_anchor()?;
            commands::agents::pause(&agent_id, printer, client, &anchor).await
        }

        Commands::Resume { agent_id } => {
            let anchor = require_anchor()?;
            commands::agents::resume(&agent_id, printer, client, &anchor).await
        }

        Commands::Kill { agent_id } => {
            let anchor = require_anchor()?;
            commands::agents::kill(&agent_id, printer, client, &anchor).await
        }

        Commands::Failover { agent_id } => {
            let anchor = require_anchor()?;
            commands::agents::failover(&agent_id, printer, client, &anchor).await
        }

        Commands::Channel { subcommand } => {
            let anchor = require_anchor()?;
            match subcommand {
                ChannelCommands::Add { kind } => match kind {
                    ChannelAddKind::Telegram {
                        token,
                        chat_ids,
                        yes,
                    } => {
                        commands::channels::add_telegram(
                            token.as_deref(),
                            &chat_ids,
                            yes,
                            printer,
                            client,
                            &anchor,
                        )
                        .await
                    }
                    ChannelAddKind::Mailbox { name } => {
                        commands::channels::add_mailbox(&name, printer, client, &anchor).await
                    }
                },
                ChannelCommands::List => commands::channels::list(printer, client, &anchor).await,
                ChannelCommands::Status { name } => {
                    commands::channels::channel_status(&name, printer, client, &anchor).await
                }
                ChannelCommands::Remove { name, yes } => {
                    commands::channels::remove(&name, yes, printer, client, &anchor).await
                }
            }
        }

        Commands::Status => {
            let anchor = require_anchor()?;
            commands::observe::status(printer, client, &anchor).await
        }

        Commands::Ui => {
            let anchor = require_anchor()?;
            commands::ui::run(printer, client, &anchor).await
        }

        Commands::Logs {
            agent_id,
            lines,
            follow,
        } => {
            let anchor = require_anchor()?;
            commands::observe::logs(&agent_id, Some(lines), follow, printer, client, &anchor).await
        }

        Commands::Message { subcommand } => {
            let anchor = require_anchor()?;
            match subcommand {
                MessageCommands::Send {
                    to_agent_id,
                    message,
                    from_agent_id,
                    kind,
                } => {
                    commands::messages::send(
                        &to_agent_id,
                        &message,
                        from_agent_id,
                        kind,
                        printer,
                        client,
                        &anchor,
                    )
                    .await
                }
                MessageCommands::Inbox { agent_id } => {
                    commands::messages::inbox(&agent_id, printer, client, &anchor).await
                }
                MessageCommands::List { agent_id } => {
                    commands::messages::list(agent_id.as_deref(), printer, client, &anchor).await
                }
            }
        }

        Commands::Clarify { subcommand } => {
            let anchor = require_anchor()?;
            match subcommand {
                ClarifyCommands::Request {
                    agent_id,
                    question,
                    task_id,
                    priority,
                    context,
                    wait: wait_for_answer,
                } => {
                    commands::clarify::request(
                        &agent_id,
                        task_id.as_deref(),
                        &question,
                        context.as_deref(),
                        priority,
                        wait_for_answer,
                        printer,
                        client,
                        &anchor,
                    )
                    .await
                }
                ClarifyCommands::List { agent_id } => {
                    commands::clarify::list(agent_id.as_deref(), printer, client, &anchor).await
                }
                ClarifyCommands::Show { request_id } => {
                    commands::clarify::show(&request_id, printer, client, &anchor).await
                }
                ClarifyCommands::Answer {
                    request_id,
                    answer,
                    payload,
                } => {
                    commands::clarify::answer(
                        &request_id,
                        &answer,
                        payload.as_deref(),
                        printer,
                        client,
                        &anchor,
                    )
                    .await
                }
                ClarifyCommands::Wait {
                    request_or_agent_id,
                    timeout_secs,
                } => {
                    commands::clarify::wait_for_response(
                        &request_or_agent_id,
                        timeout_secs,
                        printer,
                        client,
                        &anchor,
                    )
                    .await
                }
            }
        }

        Commands::Config { subcommand } => match subcommand {
            ConfigCommands::Validate => {
                let anchor = require_anchor()?;
                commands::config::validate(&anchor, printer)
            }
            ConfigCommands::Show => {
                let anchor = require_anchor()?;
                commands::config::show(printer, client, &anchor).await
            }
        },

        Commands::Taskflow { subcommand } => {
            let anchor = require_anchor()?;
            match subcommand {
                TaskflowCommands::Status => {
                    commands::taskflow::status(printer, client, &anchor).await
                }
                TaskflowCommands::List => commands::taskflow::list(printer, client, &anchor).await,
                TaskflowCommands::Show { milestone_id } => {
                    commands::taskflow::show(&milestone_id, printer, client, &anchor).await
                }
                TaskflowCommands::Sync => commands::taskflow::sync(printer, client, &anchor).await,
                TaskflowCommands::Assign {
                    roadmap_id,
                    task_id,
                } => {
                    commands::taskflow::assign(&roadmap_id, &task_id, printer, client, &anchor)
                        .await
                }
                TaskflowCommands::CreateMilestone { id, name, lld } => {
                    commands::taskflow::create_milestone(
                        &id,
                        &name,
                        lld.as_deref(),
                        printer,
                        client,
                        &anchor,
                    )
                    .await
                }
                TaskflowCommands::AddTask {
                    milestone_id,
                    id,
                    task,
                    bug,
                    maint,
                } => {
                    let m_id = milestone_id.unwrap_or_else(|| "backlog".to_string());
                    let task_type = if bug {
                        aegis_taskflow::model::TaskType::Bug
                    } else if maint {
                        aegis_taskflow::model::TaskType::Maintenance
                    } else {
                        aegis_taskflow::model::TaskType::Feature
                    };

                    commands::taskflow::add_task(
                        &m_id, &id, &task, task_type, printer, client, &anchor,
                    )
                    .await
                }
                TaskflowCommands::SetTaskStatus {
                    milestone_id,
                    task_id,
                    status,
                } => {
                    commands::taskflow::set_task_status(
                        &milestone_id,
                        &task_id,
                        &status,
                        printer,
                        client,
                        &anchor,
                    )
                    .await
                }
                TaskflowCommands::Next => {
                    commands::taskflow::next(printer, client, &anchor).await
                }
            }
        }

        Commands::Design { subcommand } => {
            let anchor = require_anchor()?;
            match subcommand {
                DesignCommands::List => commands::design::list(printer, &anchor),
                DesignCommands::Show { name } => commands::design::show(&name, printer, &anchor),
                DesignCommands::Spawn { name, model, vars } => {
                    commands::design::spawn(&name, model.as_deref(), &vars, printer, client, &anchor).await
                }
                DesignCommands::Apply { name, role, vars } => {
                    commands::design::apply(&name, role.as_deref(), &vars, printer, &anchor)
                }
                DesignCommands::New { name, kind } => {
                    commands::design::new(&name, &kind, printer, &anchor)
                }
            }
        }

        Commands::Completions { shell } => commands::completions::run::<Cli>(shell),
    }
}

fn require_anchor() -> Result<ProjectAnchor, AegisCliError> {
    let cwd = std::env::current_dir().map_err(AegisCliError::Io)?;
    ProjectAnchor::discover(&cwd)
}
