mod config;
mod handler;
mod instance;

use std::env;

use config::Config;
use serenity::{prelude::GatewayIntents, Client};

use crate::handler::Handler;

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() {
    // init logger
    log4rs::init_file("./log4rs.yml", Default::default()).unwrap();

    let cfg_path = env::var("CONFIG_PATH").unwrap_or_else(|_| String::from("./config.toml"));
    let cfg = Config::from_path(&cfg_path);

    if log::max_level().ge(&log::LevelFilter::Trace) {
        log::trace!("Generated Config from {}: {:#?}", cfg_path, cfg);
    }

    let client = Client::builder(&cfg.bot_token, GatewayIntents::empty());
    let handler = Handler::new(cfg);

    // Build our client.
    let mut client = client
        .event_handler_arc(handler)
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        log::error!("Client error: {:?}", why);
    }
}
