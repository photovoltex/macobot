use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "kebab-case"))]
pub struct Instance {
    pub cmd_exec_dir: Option<String>,
    pub cmd_path: String,
    pub cmd_args: Option<Vec<String>>,
    pub startup: StartupConfig,
    pub restrictions: RestrictionConfig,
    pub slash_commands: HashMap<String, SlashCommandConfig>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "kebab-case"))]
pub struct StartupConfig {
    pub time_to_wait: u64,
    pub wait_for_stdout: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "kebab-case"))]
pub struct RestrictionConfig {
    pub server_id: u64,
    pub fallback_channel_id: u64,
    pub allowed_channel_ids: Option<Vec<u64>>,
    pub allowed_user_ids: Option<Vec<u64>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SlashCommandConfig {
    pub description: String,
    pub stdin: Option<StdinConfig>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "kebab-case"))]
pub struct StdinConfig {
    pub cmd: String,
    // todo: make optional and impl default response
    pub interaction_msg: String,
}
