use std::collections::HashMap;

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
    InsertActiveInstance(String, ActiveInstance),
}

pub struct ActiveInstance {
    sender: Sender<InstanceInEvents>,
    channel: ChannelId,
}

pub struct Handler {
    cfg: Config,
    http: Http,
    active_instances: HashMap<String, ActiveInstance>,
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

    pub fn new(cfg: Config) -> Handler {
        let http = Http::new(&cfg.bot_token);
        let (sender, receiver) = mpsc::channel::<HandlerEvents>(5);
        let mut handler = Handler {
            cfg,
            http,
            active_instances: HashMap::new(),
            sender,
        };

        // start async receiver_thread
        tokio::runtime::Builder::new_multi_thread()
            .thread_name("Handler")
            .build()
            .expect("Couldn't build runtime to execute the async receiver thread.")
            .block_on(handler.spawn_receiver_thread(receiver));

        handler
    }

    pub async fn spawn_receiver_thread(&mut self, receiver: Receiver<HandlerEvents>) {
        let mut receiver = receiver;

        let rt = tokio::runtime::Builder::new_multi_thread()
            .thread_name("SendMessage")
            .build()
            .expect("Couldn't build multithread runtime for sending discord messages.");

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
                            todo!("ExecuteStdinCommandFailure: {err}")
                        }
                        InstanceOutEvents::StartupTimeoutFinished(instance_name, msg) => {
                            log::debug!(
                                "[{instance_name}] Startup timeout finished. Sending [{msg}]."
                            );

                            let channel = if let Some(instance) =
                                self.active_instances.get(&instance_name)
                            {
                                Some(instance.channel)
                            } else if let Some(instance) = self.cfg.instances.get(&instance_name) {
                                Some(ChannelId(instance.bot.fallback_channel_id))
                            } else {
                                log::error!("Couldn't retrieve any channel for InstanceOutEvent::StartupTimeoutFinished.");
                                None
                            };

                            if let Some(channel) = channel {
                                rt.block_on(self.send_discord_message(channel, instance_name));
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
                Some(HandlerEvents::InsertActiveInstance(string, active_instance)) => {
                    self.active_instances.insert(string, active_instance);
                }
                None => todo!("None, so channel is closed... handle in next revision"),
            };
        }
    }

    async fn send_discord_message(&self, channel: ChannelId, msg: String) {
        let res = channel
            .send_message(&self.http, |m| m.content(msg).tts(true))
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
                                    if let Err(err) = self
                                        .sender
                                        .send(HandlerEvents::InsertActiveInstance(
                                            instance_name.to_string(),
                                            ActiveInstance {
                                                sender: InstanceRunner::new(
                                                    instance_name.to_string(),
                                                    instance.clone(),
                                                    self.sender.clone(),
                                                ),
                                                channel: command.channel_id,
                                            },
                                        ))
                                        .await
                                    {
                                        log::warn!("Error inserting into active instance due to failed sending. Err: {err}");
                                    };

                                    format!("Started instance: `{instance_name}`. Will send a message after command startup.").to_string()
                                }
                                "stop" => {
                                    let res = self
                                        .active_instances
                                        .get(&instance_name.to_string())
                                        .expect("!get")
                                        .sender
                                        .send(InstanceInEvents::ExecuteStdinCommand(
                                            slash_cmd.stdin_cmd.as_ref().unwrap().to_string(),
                                        ));
                                    // use slash_cmd.stdin_cmd here
                                    if let Err(err) = res.await {
                                        err.to_string()
                                    } else {
                                        format!("Stopped {instance_name}.").to_string()
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
