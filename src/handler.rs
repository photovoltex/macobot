use std::collections::HashMap;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;

use serenity::async_trait;
use serenity::model::application::interaction::{Interaction, InteractionResponseType};
use serenity::model::gateway::Ready;
use serenity::model::id::GuildId;
use serenity::prelude::*;

use crate::config::Config;
use crate::instance::{self, ChannelEventsIn, ChannelEventsOut};

pub type ThreadSaveSyncSender = Arc<Mutex<SyncSender<ChannelEventsOut>>>;
pub type MutableHashMap = Arc<Mutex<HashMap<String, SyncSender<ChannelEventsIn>>>>;

pub struct Handler {
    cfg: Config,
    running_instances: MutableHashMap,
    sync_sender: ThreadSaveSyncSender,
}

impl Handler {
    const CMD_NAME_SEPARATOR: &'static str = "_";

    pub fn new(cfg: Config) -> Handler {
        let (sync_sender, receive_out) = sync_channel::<ChannelEventsOut>(3);

        let sync_sender: ThreadSaveSyncSender = Arc::new(Mutex::new(sync_sender));

        thread::spawn(move || loop {
            // todo: process events
            match receive_out.recv() {
                Ok(ChannelEventsOut::StoppedSuccess) => todo!("StopSuccess"),
                Ok(ChannelEventsOut::StoppedError(err)) => todo!("{err}"),
                Ok(ChannelEventsOut::ExecuteStdinCommandFailure(err)) => todo!("{err}"),
                Ok(ChannelEventsOut::ProcessTimeoutFinished(msg)) => todo!("{msg}"),
                Ok(ChannelEventsOut::Logging(msg)) => println!("{msg}"),
                Err(err) => todo!("{err}"),
            };
        });

        Handler {
            cfg,
            running_instances: Arc::new(Mutex::new(HashMap::new())),
            sync_sender,
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
            println!("Received command interaction: {:#?}", command);

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
                                "start" => match instance.run(self.sync_sender.to_owned()) {
                                    Ok(channel) => {
                                        self.running_instances
                                            .lock()
                                            .expect("!lock")
                                            .insert(instance_name.to_string(), channel);
                                        format!("Starting [{instance_name}]. I will send a message when the executed command finished starting.").to_string()
                                    }
                                    Err(err) => err,
                                },
                                "stop" => {
                                    // use slash_cmd.stdin_cmd here
                                    "not implemented :(".to_string()
                                }
                                _ => "not implemented :(".to_string(),
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
                println!("Cannot respond to slash command: {}", why);
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

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
                println!("{:#?}", commands);

                commands
            })
            .await;

            println!(
                "I now have the following guild slash commands: {:#?}",
                commands
            );
        }

        // let guild_command = Command::create_global_application_command(&ctx.http, |command| {
        //     commands::wonderful_command::register(command)
        // })
        // .await;

        // println!("I created the following global slash command: {:#?}", guild_command);
    }
}
