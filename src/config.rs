use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SlashCommandConfig {
    pub description: String,
    pub stdin_cmd: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "kebab-case"))]
pub struct StartupConfig {
    pub msg: String,
    pub time_to_wait: u64,
    pub wait_for_stdout: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "kebab-case"))]
pub struct BotConfig {
    pub server_id: u64,
    pub fallback_channel_id: u64,
    pub allowed_channel_ids: Option<Vec<u64>>,
    pub allowed_user_ids: Option<Vec<u64>>,
    pub shutdown_msg: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "kebab-case"))]
pub struct Instance {
    pub cmd_exec_dir: Option<String>,
    pub cmd_path: String,
    pub cmd_args: Vec<String>,
    pub startup: StartupConfig,
    pub slash_commands: HashMap<String, SlashCommandConfig>,
    pub bot: BotConfig,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "kebab-case"))]
pub struct Config {
    pub bot_token: String,
    #[serde(flatten)]
    pub instances: HashMap<String, Instance>,
}

impl Config {
    pub fn from_path(path: &str) -> Config {
        confy::load_path::<Config>(path).unwrap()
    }
}
