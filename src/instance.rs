use std::fmt::Display;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Stdio;
use std::process::{Child, Command};
use std::str;
use std::sync::Arc;
use std::time::Instant;

use serenity::prelude::Mutex;
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::config::Instance;
use crate::handler::HandlerEvents;

pub enum InstanceInEvents {
    ExecuteStdinCommand(String),
}

#[derive(Debug)]
pub enum InstanceOutEvents {
    ChangeDirFailure,
    Stopped,
    StoppedWithError(String),
    StdoutInitializingFailure,
    StartupTimeoutFinished(String, String),
    ExecuteStdinCommandFailure(String),
}

impl Display for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{} {}]", self.cmd_path, self.cmd_args.join(" "))
    }
}

pub struct InstanceRunner {
    name: String,
    instance: Instance,
    reached_timeout: bool,
}

impl InstanceRunner {
    pub fn new(
        name: String,
        instance: Instance,
        sender_out: Sender<HandlerEvents>,
    ) -> Sender<InstanceInEvents> {
        log::trace!("Created new InstanceRunner");
        let (sender, receiver_in) = mpsc::channel::<InstanceInEvents>(5);
        let runner = InstanceRunner {
            name,
            instance,
            reached_timeout: false,
        };

        tokio::spawn(async move {
            log::trace!("Spawned runner thread for child.");
            let (child, stdout) = runner.spawn_child(&sender_out).await;
            runner
                .run_loop(child, stdout, sender_out, receiver_in)
                .await;
            log::trace!("Finished runner thread for child.")
        });

        sender
    }

    async fn spawn_child(&self, send_out: &Sender<HandlerEvents>) -> (Child, Arc<Mutex<Vec<u8>>>) {
        log::trace!("Started spawn_child");
        // set path if given var is available
        if let Some(path) = &self.instance.cmd_exec_dir {
            if let Err(err) = std::env::set_current_dir(Path::new(&path)) {
                if let Err(send_err) = send_out
                    .send(HandlerEvents::InstanceOutEvent(
                        InstanceOutEvents::ChangeDirFailure,
                    ))
                    .await
                {
                    panic!("Failed to change current directory and sending error message. Err: {err}, SendErr: {send_err}")
                } else {
                    panic!("Failed to change current directory. Err: {err}")
                }
            }
        }

        log::trace!("Spawn child");
        // start child process
        let mut child = Command::new(&self.instance.cmd_path)
            .args(&self.instance.cmd_args.clone())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to execute child");

        // todo: maybe also get stderr and stream and analyze it
        let out = match child.stdout.take() {
            Some(stdout) => {
                log::trace!("Collecting child_stream_as_vec");
                Self::child_stream_to_vec(stdout).await
            },
            None => {
                if let Err(err) = send_out
                    .send(HandlerEvents::InstanceOutEvent(
                        InstanceOutEvents::StdoutInitializingFailure,
                    ))
                    .await
                {
                    panic!("Couldn't retrieve stdout from spawned child and sending error message. Err: {err}")
                } else {
                    panic!("Couldn't retrieve stdout from spawned child.")
                }
            }
        };

        (child, out)
    }

    async fn run_loop(
        mut self,
        child: Child,
        stdout: Arc<Mutex<Vec<u8>>>,
        send_out: Sender<HandlerEvents>,
        receiver_in: Receiver<InstanceInEvents>,
    ) {
        let mut child = child;
        let mut receiver = receiver_in;

        let mut now: Instant = Instant::now();
        let mut last_elapsed_sec = now.elapsed().as_secs();

        loop {
            if let Ok(Some(status)) = child.try_wait() {
                log::debug!("Child-Process: {} finished with: {}", self.instance, status);

                let res = if !status.success() {
                    send_out
                        .send(HandlerEvents::InstanceOutEvent(
                            InstanceOutEvents::StoppedWithError(status.to_string()),
                        ))
                        .await
                } else {
                    send_out
                        .send(HandlerEvents::InstanceOutEvent(InstanceOutEvents::Stopped))
                        .await
                };

                if let Err(send_err) = res {
                    panic!(
                        "Couldn't send stopped message to HandlerEvents. {}",
                        send_err
                    )
                } else {
                    return;
                }
            }

            if let Some(event) = receiver.recv().await {
                match event {
                    InstanceInEvents::ExecuteStdinCommand(cmd) => {
                        if let Err(err) = child
                            .stdin
                            .as_mut()
                            .expect("Couldn't retrieve stdin from spawned child.")
                            .write(format!("{cmd}\n").as_bytes())
                        {
                            if let Err(err) = send_out
                                .send(HandlerEvents::InstanceOutEvent(
                                    InstanceOutEvents::ExecuteStdinCommandFailure(err.to_string()),
                                ))
                                .await
                            {
                                log::error!("Error during sending [InstanceOutEvents::ExecuteStdinCommandFailure]. Err {err}")
                            };
                        };
                    }
                }
            }
            // todo: consider if await receives None (happened after second execution of the command, while the first didn't started the child for some reason (see stream as vec))

            let current_elapsed = now.elapsed().as_secs();
            let mut stream = stdout.lock().await;

            if let Ok(converted_stream) = str::from_utf8(&stream.to_owned()) {
                if let Some(newline_position) = converted_stream.find("\n") {
                    // remove line with newline from stream
                    stream.drain(..(newline_position + 1));

                    // possible position for logging the streamed lines
                    // let split = converted_stream.split("\n").collect::<Vec<&str>>();
                    // split.get(0).unwrap() is the last line, everything afterwards are new unfinished lines
                    let split = converted_stream.split("\n").collect::<Vec<&str>>();
                    log::debug!("{}", split.get(0).unwrap());

                    if self.instance.startup.wait_for_stdout {
                        now = Instant::now();
                    }
                } else if !self.reached_timeout {
                    if current_elapsed > last_elapsed_sec {
                        last_elapsed_sec = current_elapsed;
                        log::debug!("{current_elapsed}s");
                    } else if current_elapsed > self.instance.startup.time_to_wait {
                        if let Err(err) = send_out
                            .send(HandlerEvents::InstanceOutEvent(
                                InstanceOutEvents::StartupTimeoutFinished(
                                    self.name.to_string(),
                                    self.instance.startup.msg.to_owned(),
                                ),
                            ))
                            .await
                        {
                            log::error!("Error during sending [InstanceOutEvents::StartupTimeoutFinished]. Err: {err}")
                        };
                        // reset timer
                        now = Instant::now();
                        self.reached_timeout = true;
                    }
                }
            }
        }
    }

    // fixme: has some weird behavior, which only captures the child correct on the second run
    /// https://stackoverflow.com/a/34616729/10386701
    /// Pipe streams are blocking, we need separate threads to monitor them without blocking the primary thread.
    async fn child_stream_to_vec<R>(mut stream: R) -> Arc<Mutex<Vec<u8>>>
    where
        R: Read + Send + 'static,
    {
        let out = Arc::new(Mutex::new(Vec::new()));
        let vec = out.clone();

        tokio::spawn(async move {
            loop {
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
                            vec.lock().await.push(buf[0])
                        } else {
                            log::error!("{}] Unexpected number of bytes: {}", line!(), got);
                            break;
                        }
                    }
                }
            }
        });

        out
    }
}
