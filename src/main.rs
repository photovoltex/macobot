mod config;
mod external;

use config::Config;

use crate::external::ExternalCommandBuilder;

fn main() -> Result<(), String> {
    let path = "F:\\Users\\photovoltex\\Code\\repos\\manager-bot-rs\\test\\docs\\config.toml";

    let cfg = Config::from_path(path);
    println!("{:#?}", cfg);

    // for (name, instance) in cfg.instances {
    let (name, instance) = cfg.instances.get_key_value(&"vanilla-mc".to_string()).unwrap();
    
    let name = name.to_owned();
    let instance = instance.to_owned();

    println!("Starting {}", name);
    let builder = ExternalCommandBuilder::new(instance.cmd_path)
        .set_args(instance.cmd_args)
        .set_wait_for_stdout(instance.startup.wait_for_stdout)
        .set_time_to_wait(instance.startup.time_to_wait);

    match instance.cmd_exec_dir {
        Some (cmd_dir) => builder.set_dir(cmd_dir).expect("coulnd't change directory"),
        _ => builder
    }.run()
    // }
}
