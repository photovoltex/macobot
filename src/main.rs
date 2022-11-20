mod config;
mod instance;

use config::Config;

fn main() -> Result<(), String> {
    let path = "F:\\Users\\photovoltex\\Code\\repos\\manager-bot-rs\\test\\process\\config.toml";

    let cfg = Config::from_path(path);
    println!("{:#?}", cfg);

    // for (name, instance) in cfg.instances {
    let (name, instance) = cfg
        .instances
        .get_key_value(&"vanilla-mc".to_string())
        .unwrap();

    let name = name.to_owned();
    let instance = instance.to_owned();

    println!("Starting {}", name);

    match instance.cmd_exec_dir.to_owned() {
        Some(cmd_dir) => instance
            .set_dir(cmd_dir)
            .expect("coulnd't change directory"),
        _ => instance,
    }
    .run()
    // }
}
