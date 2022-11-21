use std::fmt::Display;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;
use std::process::Stdio;
use std::str;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use crate::config::Instance;
use crate::handler::ThreadSaveSyncSender;

pub enum ChannelEventsIn {
    ExecuteStdinCommand(String),
}

pub enum ChannelEventsOut {
    StoppedSuccess,
    ExecuteStdinCommandFailure(String),
    StoppedError(String),
    ProcessTimeoutFinished(String),
}

impl Display for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{} {}]", self.cmd_path, self.cmd_args.join(" "))
    }
}

impl Instance {
    fn stringify_io_error(x: std::io::Error) -> String {
        format!("{x}")
    }

    pub fn run(
        &self,
        name: String,
        send_out: ThreadSaveSyncSender,
    ) -> Result<SyncSender<ChannelEventsIn>, String> {
        if let Some(path) = &self.cmd_exec_dir {
            std::env::set_current_dir(Path::new(&path)).map_err(Instance::stringify_io_error)?;
        }

        // start child process
        let mut child = Command::new(self.cmd_path.clone())
            .args(self.cmd_args.clone())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to execute child");

        let out: Arc<Mutex<Vec<u8>>> =
            Instance::child_stream_to_vec(child.stdout.take().expect("!stdout"));
        // todo: maybe also get stderr and stream and analyze it

        let mut now: Instant = Instant::now();
        // fixme: hard coded buffer size for SyncSender
        let (send_in, receive_in) = sync_channel::<ChannelEventsIn>(3);

        let instance = self.to_owned();

        // todo: maybe use the handle for something
        thread::Builder::new().name(name).spawn(move || {
            loop {
                let sender = send_out.lock().expect("!lock");

                if let Ok(Some(status)) = child.try_wait() {
                    // todo: should be "printed" somewhere else
                    log::debug!("Child-Process: {} finished with: {}", instance, status);
                    if !status.success() {
                        sender.send(ChannelEventsOut::StoppedError(status.to_string()));
                    } else {
                        sender.send(ChannelEventsOut::StoppedSuccess);
                    }
                }

                if let Ok(event) = receive_in.recv() {
                    match event {
                        ChannelEventsIn::ExecuteStdinCommand(cmd) => {
                            if let Err(err) = child
                                .stdin
                                .as_mut()
                                .expect("!stdin")
                                .write(format!("{cmd}\n").as_bytes())
                            {
                                sender.send(ChannelEventsOut::ExecuteStdinCommandFailure(
                                    err.to_string(),
                                ));
                            };
                        }
                    }
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
                            log::debug!("{}", split.get(0).unwrap());

                            if instance.startup.wait_for_stdout {
                                now = Instant::now();
                            }
                        } else if now.elapsed().as_secs() > instance.startup.time_to_wait {
                            sender.send(ChannelEventsOut::ProcessTimeoutFinished(
                                instance.startup.msg.to_owned(),
                            ));
                            // reset timer
                            now = Instant::now();
                        }
                    }
                }
            }
        });

        Ok(send_in)
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
                        log::error!("{}] Error reading from stream: {}", line!(), err);
                        break;
                    }
                    Ok(got) => {
                        if got == 0 {
                            break;
                        } else if got == 1 {
                            vec.lock().expect("!lock").push(buf[0])
                        } else {
                            log::error!("{}] Unexpected number of bytes: {}", line!(), got);
                            break;
                        }
                    }
                }
            })
            .expect("!thread");
        out
    }
}
