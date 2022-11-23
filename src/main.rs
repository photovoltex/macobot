mod config;
mod handler;
mod instance;

use std::{env, sync::Arc};

use config::Config;
use serenity::{prelude::GatewayIntents, Client};

use crate::handler::Handler;

#[tokio::main]
async fn main() {
    let log_cfg_path = env::var("LOG_CONFIG_PATH").unwrap_or(String::from("./log4rs.yml"));
    log4rs::init_file(log_cfg_path, Default::default()).unwrap();

    let cfg = match env::var("CONFIG_PATH") {
        Ok(path) => {
            log::debug!("CONFIG_PATH was given. Will use {path} for config initialization.");
            Config::from_path(&path)
        }
        Err(_) => {
            log::warn!(
                "env::var CONFIG_PATH was not defined, falling back to config in current directory"
            );
            Config::from_path("./config.toml")
        }
    };

    if log::max_level().ge(&log::LevelFilter::Trace) {
        log::trace!("{:#?}", cfg);
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .thread_name("ReadChannelEventsOut")
        .build()
        .unwrap();

    let client = Client::builder(&cfg.bot_token, GatewayIntents::empty());

    let handler = Arc::new(Handler::new(cfg));
    let thread_handler = handler.clone();

    rt.spawn(async move { thread_handler.run().await });

    // Build our client.
    let mut client = client
        .event_handler_arc(handler)
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        log::error!("Client error: {:?}", why);
    }
}
