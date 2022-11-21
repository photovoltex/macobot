mod config;
mod handler;
mod instance;

use std::env;

use config::Config;
use serenity::{prelude::GatewayIntents, Client};

use crate::handler::Handler;

#[tokio::main]
async fn main() {
    let log_cfg_path = env::var("LOG_CONFIG_PATH").unwrap_or(String::from("./log4rs.yml"));
    log4rs::init_file(log_cfg_path, Default::default()).unwrap();

    let cfg_path = env::var("CONFIG_PATH").unwrap_or(String::from("./config.toml"));
    let cfg = Config::from_path(&cfg_path);
    log::trace!("{:#?}", cfg);

    // Build our client.
    let mut client = Client::builder(cfg.bot_token.to_owned(), GatewayIntents::empty())
        .event_handler(Handler::new(cfg))
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        log::error!("Client error: {:?}", why);
    }
}
