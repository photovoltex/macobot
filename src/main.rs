use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::thread;
use std::str;
use std::time::Instant;
use std::{process::Command};
// use std::sync::mpsc::channel;
use std::io::{Read, Write};

fn main() {
    // let (tx, rx) = channel();

    // let run_thread = thread::spawn(move || {
        let java = "/usr/lib/jvm/java-17-openjdk/bin/java";
        let mc_server_directory = "/home/photovoltex/Repositories/spawn-child-rs/process/";

        assert!(std::env::set_current_dir(Path::new(mc_server_directory)).is_ok());

        let mut child = Command::new(java)
            .args(["-Xmx1024M", "-Xms1024M", "-jar", "server.jar", "nogui"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to execute child");

        let mut now: Instant = Instant::now();

        let out = child_stream_to_vec(child.stdout.take().expect("!stdout"));

        let mut last_elapsed_time = now.elapsed().as_secs();
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                println!("{}", status);
                return
            }

            let str = out.lock().expect("!lock").to_owned();
            let str = str::from_utf8(&str).expect("!from_utf8");

            if let Some(newline_position) = str.find("\n") {
                now = Instant::now();

                let split = str.split("\n").collect::<Vec<&str>>();

                println!("{}", split.get(0).unwrap());

                out.lock().expect("!lock").drain(..(newline_position + 1));
            } else if !str.is_empty() {
                //// debug output line
                // print!("{:?}", str)
            } else {
                let elapsed_sec = now.elapsed().as_secs();

                if last_elapsed_time.ne(&elapsed_sec) {
                    println!("{}s ", elapsed_sec);
                    last_elapsed_time = elapsed_sec;
                }

                if elapsed_sec > 10 {
                    println!("SERVER IS ONLINE");
                    match child.stdin.as_mut().expect("!stdin").write("stop\n".as_bytes()) {
                        Ok(res) => println!("{}", res),
                        Err(err) => println!("{}", err),
                    };
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
