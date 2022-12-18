mod async_trait;

use std::collections::HashMap;
use std::sync::Arc;

use serenity::http::Http;
use serenity::model::prelude::ChannelId;
use serenity::prelude::*;
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::config::bot;
use crate::instance::{InstanceInEvents, InstanceOutEvents};

pub enum HandlerEvents {
    InstanceOutEvent(InstanceOutEvents),
    ErrorOnSendingDiscordMessage(String),
}

pub struct ActiveInstance {
    pub sender: Sender<InstanceInEvents>,
    pub channel: ChannelId,
}

pub struct Handler {
    pub cfg: bot::Config,
    http: Http,
    pub active_instances: Arc<Mutex<HashMap<String, ActiveInstance>>>,
    pub sender: Sender<HandlerEvents>,
}

impl Handler {
    const CMD_NAME_SEPARATOR: &'static str = "-";

    pub fn new(cfg: bot::Config) -> Arc<Handler> {
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

    pub fn make_cmd_name(instance_name: &String, slash_cmd_name: &String) -> String {
        format!(
            "{}{}{}",
            slash_cmd_name,
            Handler::CMD_NAME_SEPARATOR,
            instance_name
        )
    }

    pub fn separat_cmd_name(cmd_name: &str) -> Result<(&str, &str), String> {
        let splitted: Vec<&str> = cmd_name.split(Handler::CMD_NAME_SEPARATOR).collect();

        if splitted.len() < 2 {
            return Err(format!(
                "Split on cmd name with separator ({}) resulted in less then 2.",
                Handler::CMD_NAME_SEPARATOR
            ));
        }

        let mut err: Option<String> = None;
        let mut name: Option<&str> = None;
        let mut instance: Option<&str> = None;

        for i in 0..1 {
            match splitted.get(i) {
                Some(item) => {
                    if i == 0 {
                        name = Some(item);
                    } else {
                        instance = Some(item);
                    }
                }
                None => {
                    err = if let Some(err) = err {
                        Some(format!("Couldn't retrieve {i}. {err}"))
                    } else {
                        Some(format!("Couldn't retrieve {i}."))
                    };
                }
            }
        }

        if let Some(why) = err {
            Err(why)
        } else if let Some(name) = name {
            if let Some(instance) = instance {
                Ok((name, instance))
            } else {
                Err(String::from("Unexpected error during cmd name separation"))
            }
        } else {
            Err(String::from("Unexpected error during cmd name separation"))
        }
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
                        InstanceOutEvents::Stopped(instance_name) => {
                            log::debug!(
                                "[{instance_name}] Stopping finished. Sending stopped message."
                            );
                            Self::send_discord_message_to_instance_channel(
                                &handler,
                                &instance_name,
                                format!("Stopped `{instance_name}`"),
                            )
                            .await;
                        }
                        InstanceOutEvents::StoppedWithError(err) => todo!("StoppedError: {err}"),
                        InstanceOutEvents::ExecuteStdinCommandFailure(err) => {
                            todo!("ExecuteStdinCommandFailure: {err}")
                        }
                        InstanceOutEvents::StartupTimeoutFinished(instance_name) => {
                            log::debug!("[{instance_name}] Startup timeout finished. Sending startup message.");
                            Self::send_discord_message_to_instance_channel(
                                &handler,
                                &instance_name,
                                format!("Started `{instance_name}`. Server/Application is up and running.")
                            ).await;
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

    async fn send_discord_message_to_instance_channel(
        handler: &Handler,
        instance_name: &str,
        msg: String,
    ) {
        let channel =
            if let Some(instance) = handler.active_instances.lock().await.get(instance_name) {
                Some(instance.channel)
            } else if let Some(instance) = handler.cfg.instances.get(instance_name) {
                Some(ChannelId(instance.restrictions.fallback_channel_id))
            } else {
                log::error!("Couldn't retrieve any active channel for `{instance_name}`.");
                None
            };

        if let Some(channel) = channel {
            handler.send_discord_message(channel, msg).await;
        }
    }

    async fn send_discord_message(&self, channel: ChannelId, msg: String) {
        let res = channel.send_message(&self.http, |m| m.content(msg)).await;
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
