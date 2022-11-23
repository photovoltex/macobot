use std::collections::HashMap;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};

use serenity::async_trait;
use serenity::http::Http;
use serenity::model::application::interaction::{Interaction, InteractionResponseType};
use serenity::model::gateway::Ready;
use serenity::model::id::GuildId;
use serenity::model::prelude::ChannelId;
use serenity::prelude::*;

use crate::config::Config;
use crate::instance::{ChannelEventsIn, ChannelEventsOut, ThreadSafeOptionalSyncSender};

pub struct ActiveInstance {
    sender: SyncSender<ChannelEventsIn>,
    channel: ChannelId,
}

pub struct Handler {
    cfg: Config,
    active_instances: Arc<Mutex<HashMap<String, ActiveInstance>>>,
    sync_sender: ThreadSafeOptionalSyncSender,
}

impl Handler {
    const CMD_NAME_SEPARATOR: &'static str = "_";

    pub fn new(cfg: Config) -> Handler {
        Handler {
            cfg,
            active_instances: Arc::new(Mutex::new(HashMap::new())),
            sync_sender: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn run(&self) {
        log::debug!("EventOut Handler is running.");

        let (sync_sender, receive_out) = sync_channel::<ChannelEventsOut>(3);
        let _ = self.sync_sender.lock().expect("!lock").insert(sync_sender);

        let copied_bot_token = self.cfg.bot_token.clone();
        let copied_active_instance = self.active_instances.clone();

        let http = Arc::new(Http::new(&copied_bot_token));

        let rt = tokio::runtime::Builder::new_multi_thread()
            .thread_name("DirectMessage")
            .build()
            .unwrap();

        loop {
            // todo: process events
            let received = receive_out.recv();
            match received {
                Ok(ChannelEventsOut::StoppedSuccess) => todo!("StopSuccess"),
                Ok(ChannelEventsOut::StoppedError(err)) => todo!("StoppedError: {err}"),
                Ok(ChannelEventsOut::ExecuteStdinCommandFailure(err)) => {
                    todo!("ExecuteStdinCommandFailure: {err}")
                }
                Ok(ChannelEventsOut::StartupTimeoutFinished(instance_name, msg)) => {
                    log::debug!("Startup timeout finished for {instance_name} - {msg}");

                    let active_instance = copied_active_instance.clone();
                    let async_http = http.clone();

                    rt.spawn(async move {
                        log::debug!("!plz speak");
                        let channel = active_instance
                            .lock()
                            .expect("!lock")
                            .get(&instance_name)
                            .expect("!get")
                            .channel;

                        let res = channel
                            .send_message(&async_http, |m| m.content(msg).tts(true))
                            .await;
                        println!("{:?}", res);
                    });
                }
                Err(err) => todo!("Error: {err}"),
            };
        }
    }

    fn make_cmd_name(instance_name: &String, slash_cmd_name: &String) -> String {
        format!(
            "{}{}{}",
            instance_name,
            Handler::CMD_NAME_SEPARATOR,
            slash_cmd_name
        )
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
                                "start" => match instance
                                    .run(instance_name.to_string(), self.sync_sender.clone())
                                {
                                    Ok(sender) => {
                                        let channel = command.channel_id;

                                        self.active_instances.lock().expect("!lock").insert(
                                            instance_name.to_string(),
                                            ActiveInstance { sender, channel },
                                        );
                                        format!("Started instance: `{instance_name}`. Will send a message after command startup.").to_string()
                                    }
                                    Err(err) => err,
                                },
                                "stop" => {
                                    let res = self
                                        .active_instances
                                        .lock()
                                        .expect("!lock")
                                        .get(&instance_name.to_string())
                                        .expect("!get")
                                        .sender
                                        .send(ChannelEventsIn::ExecuteStdinCommand(
                                            slash_cmd.stdin_cmd.as_ref().unwrap().to_string(),
                                        ));
                                    // use slash_cmd.stdin_cmd here
                                    if res.is_err() {
                                        res.unwrap_err().to_string()
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
