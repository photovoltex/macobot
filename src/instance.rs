use std::fmt::Display;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Stdio;
use std::process::{Child, Command};
use std::str;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serenity::prelude::Mutex;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::sleep;

use crate::config::bot::Instance;
use crate::handler::HandlerEvents;

impl Display for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.cmd_args {
            Some(arguments) => write!(f, "[{} {}]", self.cmd_path, arguments.join(" ")),
            None => write!(f, "[{}]", self.cmd_path)
        }
    }
}

#[derive(Debug)]
pub enum InstanceInEvents {
    ExecuteStdinCommand(String),
}

#[derive(Debug)]
pub enum InstanceOutEvents {
    ChangeDirFailure,
    Stopped(String),
    StoppedWithError(String),
    StdoutInitializingFailure,
    StartupTimeoutFinished(String),
    ExecuteStdinCommandFailure(String),
}

pub struct InstanceRunner {
    name: String,
    instance: Instance,
}

impl InstanceRunner {
    pub fn new(
        name: String,
        instance: Instance,
        sender_out: Sender<HandlerEvents>,
    ) -> Sender<InstanceInEvents> {
        log::trace!("[{name}] Creating new InstanceRunner");
        let (sender, receiver_in) = mpsc::channel::<InstanceInEvents>(5);
        let runner = InstanceRunner { name, instance };

        let name = runner.name.clone();
        tokio::spawn(async move {
            log::trace!("[{}] Spawned runner thread for child", runner.name);
            let (child, stdout) = runner.spawn_child(&sender_out).await;
            runner
                .run_loop(child, stdout, sender_out, receiver_in)
                .await;
            log::trace!("[{}] Finished runner thread for child", runner.name)
        });

        log::trace!("[{name}] Created new InstanceRunner");
        sender
    }

    async fn spawn_child(&self, send_out: &Sender<HandlerEvents>) -> (Child, Arc<Mutex<Vec<u8>>>) {
        log::trace!("[{}] Started spawn_child", self.name);
        // set path if given var is available
        if let Some(path) = &self.instance.cmd_exec_dir {
            if let Err(err) = std::env::set_current_dir(Path::new(&path)) {
                if let Err(send_err) = send_out
                    .send(HandlerEvents::InstanceOutEvent(
                        InstanceOutEvents::ChangeDirFailure,
                    ))
                    .await
                {
                    panic!("[{}] Failed to change current directory and sending error message. Err: {err}, SendErr: {send_err}", self.name)
                } else {
                    panic!(
                        "[{}] Failed to change current directory. Err: {err}",
                        self.name
                    )
                }
            }
        }

        log::trace!("Spawn child");
        // start child process
        let mut child = Command::new(&self.instance.cmd_path);

        let child = if let Some(args) = self.instance.cmd_args.clone() {
            child.args(args)
        } else {
            &mut child
        };

        let mut child = child
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to execute child");

        // todo: maybe also get stderr, stream and analyze
        let out = match child.stdout.take() {
            Some(stdout) => {
                log::trace!("[{}] Collecting child_stream_as_vec", self.name);
                Self::child_stream_to_vec(stdout)
            }
            None => {
                if let Err(err) = send_out
                    .send(HandlerEvents::InstanceOutEvent(
                        InstanceOutEvents::StdoutInitializingFailure,
                    ))
                    .await
                {
                    panic!("[{}] Couldn't retrieve stdout from spawned child and sending error message. Err: {err}", self.name)
                } else {
                    panic!(
                        "[{}] Couldn't retrieve stdout from spawned child.",
                        self.name
                    )
                }
            }
        };

        (child, out)
    }

    async fn run_loop(
        &self,
        child: Child,
        stdout: Arc<Mutex<Vec<u8>>>,
        send_out: Sender<HandlerEvents>,
        receiver_in: Receiver<InstanceInEvents>,
    ) {
        let mut child = child;
        let mut receiver = receiver_in;

        let mut reached_timeout = false;
        let mut now: Instant = Instant::now();
        let mut last_elapsed_sec = now.elapsed().as_secs();

        log::trace!(
            "[{}] All prerequisites were successful. Starting run loop",
            self.name
        );
        loop {
            sleep(Duration::from_millis(100)).await;

            if let Ok(Some(status)) = child.try_wait() {
                log::debug!(
                    "[{}] Child-Process: {} finished with: {}",
                    self.name,
                    self.instance,
                    status
                );

                let res = if !status.success() {
                    send_out
                        .send(HandlerEvents::InstanceOutEvent(
                            InstanceOutEvents::StoppedWithError(status.to_string()),
                        ))
                        .await
                } else {
                    send_out
                        .send(HandlerEvents::InstanceOutEvent(InstanceOutEvents::Stopped(
                            self.name.clone(),
                        )))
                        .await
                };

                if let Err(send_err) = res {
                    panic!(
                        "[{}] Couldn't send stopped message to HandlerEvents. {}",
                        self.name, send_err
                    )
                } else {
                    // exit loop
                    return;
                }
            }

            match receiver.try_recv() {
                Ok(event) => match event {
                    InstanceInEvents::ExecuteStdinCommand(cmd) => {
                        if let Err(err) = child
                            .stdin
                            .as_mut()
                            .expect(&format!(
                                "[{}] Couldn't retrieve stdin from spawned child.",
                                self.name
                            ))
                            .write(format!("{cmd}\n").as_bytes())
                        {
                            if let Err(err) = send_out
                                .send(HandlerEvents::InstanceOutEvent(
                                    InstanceOutEvents::ExecuteStdinCommandFailure(err.to_string()),
                                ))
                                .await
                            {
                                log::error!("[{}] Error during sending [InstanceOutEvents::ExecuteStdinCommandFailure]. Err {err}", self.name)
                            };
                        };
                    }
                },
                Err(err) => {
                    if let mpsc::error::TryRecvError::Disconnected = err {
                        log::error!("[{}] Receiver was disconnected", self.name)
                    }
                }
            }

            let mut stream = stdout.lock().await;

            if let Ok(converted_stream) = str::from_utf8(&stream.to_owned()) {
                if let Some(newline_position) = converted_stream.find("\n") {
                    // remove line with newline from stream
                    stream.drain(..(newline_position + 1));

                    let split = converted_stream.split("\n").collect::<Vec<&str>>();
                    log::debug!("[{}] {}", self.name, split.get(0).unwrap());

                    if self.instance.startup.wait_for_stdout {
                        now = Instant::now();
                    }
                }
            }

            if !reached_timeout {
                let current_elapsed = now.elapsed().as_secs();

                if current_elapsed > last_elapsed_sec {
                    last_elapsed_sec = current_elapsed;
                    log::trace!("{current_elapsed}s");
                } else if current_elapsed > self.instance.startup.time_to_wait {
                    if let Err(err) = send_out
                        .send(HandlerEvents::InstanceOutEvent(
                            InstanceOutEvents::StartupTimeoutFinished(self.name.to_string()),
                        ))
                        .await
                    {
                        log::error!("[{}] Error during sending [InstanceOutEvents::StartupTimeoutFinished]. Err: {err}", self.name)
                    };
                    // reset timer
                    now = Instant::now();
                    reached_timeout = true;
                }
            }
        }
    }

    /// https://stackoverflow.com/a/34616729/10386701
    /// Pipe streams are blocking, we need separate threads to monitor them without blocking the primary thread.
    fn child_stream_to_vec<R>(stream: R) -> Arc<Mutex<Vec<u8>>>
    where
        R: Read + Send + 'static,
    {
        log::trace!("Starting stream reading thread");
        let out = Arc::new(Mutex::new(Vec::new()));
        let vec = out.clone();

        tokio::spawn(Self::read_stream_loop(stream, vec));
        log::trace!("Finished starting stream reading thread");

        out
    }

    async fn read_stream_loop<R>(mut stream: R, vec: Arc<Mutex<Vec<u8>>>)
    where
        R: Read + Send + 'static,
    {
        log::trace!("Started stream reading thread");
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
    }
}
