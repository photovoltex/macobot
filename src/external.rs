use std::fmt::Display;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;
use std::process::Stdio;
use std::str;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

#[derive(Default, Debug)]
pub struct ExternalCommandBuilder {
    cmd: String,
    args: Vec<String>,
    log_dir: Option<String>,
    wait_for_stdout_timeout: bool,
    time_to_wait: u64,
}

impl Display for ExternalCommandBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{} {}]", self.cmd, self.args.join(" "))
    }
}

impl ExternalCommandBuilder {
    pub fn new(cmd: String) -> ExternalCommandBuilder {
        ExternalCommandBuilder {
            cmd,
            args: Vec::new(),
            log_dir: None,
            wait_for_stdout_timeout: false,
            time_to_wait: 30,
        }
    }

    pub fn set_args(mut self, mut args: Vec<String>) -> Self {
        self.args.append(&mut args);
        self
    }

    pub fn set_dir(self, dir: String) -> Result<Self, std::io::Error> {
        match std::env::set_current_dir(Path::new(&dir)) {
            Ok(_) => {
                Ok(self)
            }
            Err(err) => Err(err),
        }
    }

    pub fn set_log_dir(mut self, dir: String) -> Self {
        self.log_dir = Some(dir);
        // todo: do something more here, logger required, or something else
        self
    }

    pub fn set_wait_for_stdout(mut self, should_wait: bool) -> Self {
        self.wait_for_stdout_timeout = should_wait;
        self
    }

    pub fn set_time_to_wait(mut self, time: u64) -> Self {
        self.time_to_wait = time;
        self
    }

    pub fn run(&self) -> Result<(), String> {
        // start child process
        let mut child = Command::new(self.cmd.clone())
            .args(self.args.clone())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to execute child");

        let out: Arc<Mutex<Vec<u8>>> = child_stream_to_vec(child.stdout.take().expect("!stdout"));
        // todo: maybe also get stderr and stream and analyze it

        let mut now: Instant = Instant::now();
        let mut last_elapsed_time = now.elapsed().as_secs();

        loop {
            if let Ok(Some(status)) = child.try_wait() {
                println!("Child-Process: {} finished with: {}", self, status);
                if !status.success() {
                    todo!("impl log output or save log or something");
                    // return Err(status.to_string());
                }
                return Ok(());
            }

            if let Ok(stream) = out.lock().as_mut() {
                if let Ok(converted_stream) = str::from_utf8(&stream.to_owned()) {
                    if let Some(newline_position) = converted_stream.find("\n") {
                        if self.wait_for_stdout_timeout {
                            now = Instant::now();
                        }

                        stream.drain(..(newline_position + 1));

                        if self.log_dir.is_some() {
                            let split = converted_stream.split("\n").collect::<Vec<&str>>();
                            println!("{}", split.get(0).unwrap());
                            todo!("logging isn't supported, yet")
                        } else {
                            let elapsed_sec = now.elapsed().as_secs();

                            if last_elapsed_time.ne(&elapsed_sec) {
                                last_elapsed_time = elapsed_sec;
                            }

                            if elapsed_sec > self.time_to_wait {
                                match child
                                    .stdin
                                    .as_mut()
                                    .expect("!stdin")
                                    .write("stop\n".as_bytes())
                                {
                                    Ok(res) => println!("{}", res),
                                    Err(err) => println!("{}", err),
                                };
                                todo!("execute cmd here probably")
                            }
                        }
                    }
                }
            }
        }
    }
}

/// https://stackoverflow.com/a/34616729/10386701
/// Pipe streams are blocking, we need separate threads to monitor them without blocking the primary thread.
fn child_stream_to_vec<R>(mut stream: R) -> Arc<Mutex<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    let out = Arc::new(Mutex::new(Vec::new()));
    let vec = out.clone();
    thread::Builder::new()
        .name("child_stream_to_vec".into())
        .spawn(move || loop {
            let mut buf = [0];
            match stream.read(&mut buf) {
                Err(err) => {
                    println!("{}] Error reading from stream: {}", line!(), err);
                    break;
                }
                Ok(got) => {
                    if got == 0 {
                        break;
                    } else if got == 1 {
                        vec.lock().expect("!lock").push(buf[0])
                    } else {
                        println!("{}] Unexpected number of bytes: {}", line!(), got);
                        break;
                    }
                }
            }
        })
        .expect("!thread");
    out
}
