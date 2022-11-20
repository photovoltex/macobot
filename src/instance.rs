use std::fmt::Display;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;
use std::process::Stdio;
use std::str;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use crate::config::Instance;

impl Display for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{} {}]", self.cmd_path, self.cmd_args.join(" "))
    }
}

impl Instance {
    pub fn set_dir(self, dir: String) -> Result<Self, std::io::Error> {
        match std::env::set_current_dir(Path::new(&dir)) {
            Ok(_) => Ok(self),
            Err(err) => Err(err),
        }
    }

    pub fn run(&self) -> Result<(), String> {
        // start child process
        let mut child = Command::new(self.cmd_path.clone())
            .args(self.cmd_args.clone())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to execute child");

        let out: Arc<Mutex<Vec<u8>>> = child_stream_to_vec(child.stdout.take().expect("!stdout"));
        // todo: maybe also get stderr and stream and analyze it

        let mut now: Instant = Instant::now();
        // let mut last_elapsed_time = now.elapsed().as_secs();

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
                        // remove line with newline from stream
                        stream.drain(..(newline_position + 1));

                        // possible position for logging the streamed lines
                        // let split = converted_stream.split("\n").collect::<Vec<&str>>();
                        // split.get(0).unwrap() is the last line, everything afterwards are new unfinished lines
                        let split = converted_stream.split("\n").collect::<Vec<&str>>();
                        println!("{}", split.get(0).unwrap());

                        if self.startup.wait_for_stdout {
                            now = Instant::now();
                        }
                    } else if now.elapsed().as_secs() > self.startup.time_to_wait {
                        if let Err(err) = child
                            .stdin
                            .as_mut()
                            .expect("!stdin")
                            .write("stop\n".as_bytes())
                        {
                            println!("{}", err)
                        };

                        // todo: send bot message
                        println!("{}", self.bot.shutdown_msg);

                        // reset timer
                        now = Instant::now();
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
