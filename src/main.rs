mod config;
mod handler;
mod instance;

use std::env;

use config::Config;
use serenity::{prelude::GatewayIntents, Client};

use crate::handler::Handler;

#[tokio::main]
async fn main() {
    let path = env::var("CONFIG_PATH").unwrap_or(String::from("./config.toml"));
    let cfg = Config::from_path(&path);
    println!("{:#?}", cfg);

    // Build our client.
    let mut client = Client::builder(cfg.bot_token.to_owned(), GatewayIntents::empty())
        .event_handler(Handler::new(cfg))
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
