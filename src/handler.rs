use std::collections::HashMap;
use std::sync::Arc;

use serenity::async_trait;
use serenity::http::Http;
use serenity::model::application::interaction::{Interaction, InteractionResponseType};
use serenity::model::gateway::Ready;
use serenity::model::id::GuildId;
use serenity::model::prelude::ChannelId;
use serenity::prelude::*;
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::config::Config;
use crate::instance::{InstanceInEvents, InstanceOutEvents, InstanceRunner};

pub enum HandlerEvents {
    InstanceOutEvent(InstanceOutEvents),
    ErrorOnSendingDiscordMessage(String),
}

pub struct ActiveInstance {
    sender: Sender<InstanceInEvents>,
    channel: ChannelId,
}

pub struct Handler {
    cfg: Config,
    http: Http,
    active_instances: Arc<Mutex<HashMap<String, ActiveInstance>>>,
    sender: Sender<HandlerEvents>,
}

impl Handler {
    const CMD_NAME_SEPARATOR: &'static str = "_";

    fn make_cmd_name(instance_name: &String, slash_cmd_name: &String) -> String {
        format!(
            "{}{}{}",
            instance_name,
            Handler::CMD_NAME_SEPARATOR,
            slash_cmd_name
        )
    }

    pub fn new(cfg: Config) -> Arc<Handler> {
        let http = Http::new(&cfg.bot_token);
        let (sender, receiver) = mpsc::channel::<HandlerEvents>(5);
        let handler = Arc::new(Handler {
            cfg,
            http,
            active_instances: Arc::new(Mutex::new(HashMap::new())),
            sender,
        });

        tokio::spawn(Self::start_receiver_thread(handler.clone(), receiver));

        handler
    }

    pub async fn start_receiver_thread(handler: Arc<Self>, receiver: Receiver<HandlerEvents>) {
        log::debug!("Started receiver thread!");
        let mut receiver = receiver;

        loop {
            match receiver.recv().await {
                Some(HandlerEvents::ErrorOnSendingDiscordMessage(error_msg)) => {
                    todo!("HandlerEvents::ErrorOnSendingDiscordMessage: {error_msg}")
                }
                Some(HandlerEvents::InstanceOutEvent(instance_event_out)) => {
                    match instance_event_out {
                        InstanceOutEvents::Stopped => todo!("StopSuccess"),
                        InstanceOutEvents::StoppedWithError(err) => todo!("StoppedError: {err}"),
                        InstanceOutEvents::ExecuteStdinCommandFailure(err) => {
                            // fixme: this is currently thrown if stdin is executed
                            todo!("ExecuteStdinCommandFailure: {err}")
                        }
                        InstanceOutEvents::StartupTimeoutFinished(instance_name, msg) => {
                            log::debug!(
                                "[{instance_name}] Startup timeout finished. Sending [{msg}]."
                            );

                            let channel = if let Some(instance) =
                                handler.active_instances.lock().await.get(&instance_name)
                            {
                                Some(instance.channel)
                            } else if let Some(instance) = handler.cfg.instances.get(&instance_name)
                            {
                                Some(ChannelId(instance.bot.fallback_channel_id))
                            } else {
                                log::error!("Couldn't retrieve any channel for InstanceOutEvent::StartupTimeoutFinished.");
                                None
                            };

                            if let Some(channel) = channel {
                                handler.send_discord_message(channel, instance_name).await;
                            }
                        }
                        InstanceOutEvents::ChangeDirFailure => {
                            todo!("InstanceOutEvents::ChangeDirFailure")
                        }
                        InstanceOutEvents::StdoutInitializingFailure => {
                            todo!("InstanceOutEvents::StdoutInitializingFailure")
                        }
                    }
                }
                None => todo!("None, so channel is closed... handle in next revision"),
            };
        }
    }

    async fn send_discord_message(&self, channel: ChannelId, msg: String) {
        let res = channel
            .send_message(&self.http, |m| m.content(msg))
            .await;
        match res {
            Ok(result) => log::trace!("{:#?}", result),
            Err(err) => {
                if let Err(send_err) = self
                    .sender
                    .send(HandlerEvents::ErrorOnSendingDiscordMessage(format!(
                        "{:?}",
                        err
                    )))
                    .await
                {
                    log::error!("Error occurred during sending HandlerEvent: {}", send_err)
                }
            }
        };
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            log::trace!("Received command interaction: {:#?}", command);

            let cmd_name = command.data.name.as_str();
            let split: Vec<&str> = cmd_name.split(Handler::CMD_NAME_SEPARATOR).collect();

            let (instance_name, slash_cmd_name) = (split.get(0), split.get(1));

            let mut command_response = "not implemented :(".to_string();

            if let Some(instance_name) = instance_name {
                if let Some(slash_cmd_name) = slash_cmd_name {
                    if let Some(instance) = self.cfg.instances.get(instance_name.to_owned()) {
                        if let Some(slash_cmd) =
                            instance.slash_commands.get(slash_cmd_name.to_owned())
                        {
                            command_response = match slash_cmd_name.trim() {
                                "start" => {
                                    log::debug!("Start command received for [{instance_name}]");
                                    self.active_instances.lock().await.insert(
                                        instance_name.to_string(),
                                        ActiveInstance {
                                            sender: InstanceRunner::new(
                                                instance_name.to_string(),
                                                instance.clone(),
                                                self.sender.clone(),
                                            ),
                                            channel: command.channel_id,
                                        },
                                    );

                                    format!("Started instance: `{instance_name}`. Will send a message after command startup.").to_string()
                                }
                                "stop" => {
                                    let await_instances = self.active_instances.lock().await;

                                    if let Some(active_instance) =
                                        await_instances.get(&instance_name.to_string())
                                    {
                                        let sender_result = active_instance
                                            .sender
                                            .send(InstanceInEvents::ExecuteStdinCommand(
                                                slash_cmd.stdin_cmd.as_ref().unwrap().to_string(),
                                            ))
                                            .await;

                                        if let Err(err) = sender_result {
                                            err.to_string()
                                        } else {
                                            format!("Stopped {instance_name}.").to_string()
                                        }
                                    } else {
                                        "Execution failed due to internal error".to_string()
                                    }
                                }
                                _ => "not currently supported or implemented".to_string(),
                            };
                        }
                    }
                }
            };

            if let Err(why) = command
                .create_interaction_response(&ctx.http, |response| {
                    response
                        .kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| message.content(command_response))
                })
                .await
            {
                log::warn!("Cannot respond to slash command: {}", why);
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        log::debug!("{} is connected!", ready.user.name);

        for (instance_name, instance) in self.cfg.instances.to_owned() {
            let guild_id = GuildId(instance.bot.server_id);

            let commands = GuildId::set_application_commands(&guild_id, &ctx.http, |commands| {
                for (slash_cmd_name, slash_cmd) in instance.slash_commands {
                    let cmd_name = Handler::make_cmd_name(&instance_name, &slash_cmd_name);

                    commands.create_application_command(|command| {
                        command
                            .name(cmd_name)
                            // .dm_permission(false)
                            // todo: .default_member_permissions(Permissions::)
                            .description(slash_cmd.description)
                    });
                }
                log::trace!("{:#?}", commands);

                commands
            })
            .await;

            log::trace!(
                "I now have the following guild slash commands: {:#?}",
                commands
            );
        }
    }
}
