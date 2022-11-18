use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SlashCommand {
    pub description: String,
    pub stdin_cmd: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Startup {
    pub cmd: String,
    pub time_to_wait: u64,
    pub wait_for_stdout: bool
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "kebab-case"))]
pub struct Instance {
    pub cmd_exec_dir: Option<String>,
    pub cmd_path: String,
    pub cmd_args: Vec<String>,
    pub startup: Startup,
    pub slash_commands: HashMap<String, SlashCommand>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(flatten)]
    pub instances: HashMap<String, Instance>,
}

impl Config {
    pub fn from_path(path: &str) -> Config {
        confy::load_path::<Config>(path).unwrap()
    }
}
