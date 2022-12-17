use serenity::{
    async_trait,
    model::{
        application::interaction::{Interaction, InteractionResponseType},
        gateway::Ready,
        id::GuildId,
    },
    prelude::*,
};

use crate::{
    handler::handler::ActiveInstance,
    instance::{InstanceInEvents, InstanceRunner},
};

use super::handler::Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            log::trace!("Received command interaction: {:#?}", command);

            let cmd_name = command.data.name.as_str();

            let command_response = match Handler::separat_cmd_name(cmd_name) {
                Ok((slash_cmd_name, instance_name)) => {
                    if let Some(instance) = self.cfg.instances.get(instance_name) {
                        if let Some(slash_cmd) = instance.slash_commands.get(slash_cmd_name) {
                            match slash_cmd_name.trim() {
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
                                    format!("Starting `{instance_name}`. Will send a message after startup.")
                                }
                                _ => {
                                    // impl for own custom commands
                                    if let Some(stdin) = slash_cmd.stdin.clone() {
                                        let await_instances = self.active_instances.lock().await;

                                        if let Some(active_instance) =
                                            await_instances.get(&instance_name.to_string())
                                        {
                                            let sender_result = active_instance
                                                .sender
                                                .send(InstanceInEvents::ExecuteStdinCommand(
                                                    stdin.cmd,
                                                ))
                                                .await;

                                            if let Err(err) = sender_result {
                                                err.to_string()
                                            } else {
                                                stdin.interaction_msg.replace("{}", &instance_name)
                                            }
                                        } else {
                                            format!("There is no running instance for `{instance_name}`.")
                                        }
                                    } else {
                                        String::from("not currently supported or implemented (5)")
                                    }
                                }
                            }
                        } else {
                            String::from("not currently supported or implemented (4)")
                        }
                    } else {
                        String::from("not currently supported or implemented (3)")
                    }
                }
                Err(why) => why,
            };

            // let command_response =

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

        // todo: redesing this spot... set_app_cmd overrides all commands
        for (instance_name, instance) in self.cfg.instances.to_owned() {
            let guild_id = GuildId(instance.restrictions.server_id);

            let commands = GuildId::set_application_commands(&guild_id, &ctx.http, |commands| {
                for (slash_cmd_name, slash_cmd) in instance.slash_commands {
                    let cmd_name = Handler::make_cmd_name(&instance_name, &slash_cmd_name);

                    commands.create_application_command(|command| {
                        command.name(cmd_name).description(slash_cmd.description)
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
