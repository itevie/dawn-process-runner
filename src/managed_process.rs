use std::io::{BufRead, BufReader};
use std::os::unix::prelude::ExitStatusExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

pub struct ManagedProcess {
    pub name: String,
    pub command: Vec<String>,
    pub child: Option<Child>,
    pub logs: Arc<Mutex<Vec<String>>>,
    pub started_at: Option<Instant>,
    pub status: Option<ExitStatus>,
    pub cwd: Option<String>,
    pub port: Option<u16>,
}

impl ManagedProcess {
    pub fn new(name: &str, command: Vec<String>, cwd: Option<String>, port: Option<u16>) -> Self {
        Self {
            name: name.into(),
            command,
            cwd,
            child: None,
            logs: Arc::new(Mutex::new(Vec::new())),
            started_at: None,
            status: None,
            port
        }
    }

    pub fn start(&mut self) {
        if self.child.is_some() {
            return;
        }

        let mut cmd = Command::new(&self.command[0]);

        if let Some(ref cwd) = self.cwd {
            cmd.current_dir(cwd.clone());
        }

        if self.command.len() > 1 {
            cmd.args(&self.command[1..]);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        match cmd.spawn() {
            Ok(mut child) => {
                let logs = self.logs.clone();

                logs.lock().unwrap().push(format!("Working Directory: {:?}", self.cwd.clone()));
                logs.lock().unwrap().push(format!("Command: {}", self.command.join(" ")));
                logs.lock().unwrap().push("\n".to_string());
                logs.lock().unwrap().push("--- Start Logs ---".to_string());
                logs.lock().unwrap().push("\n".to_string());

                if let Some(stdout) = child.stdout.take() {
                    thread::spawn(move || {
                        let reader = BufReader::new(stdout);
                        for line in reader.lines() {
                            if let Ok(line) = line {
                                logs.lock().unwrap().push(line);
                            }
                        }
                    });
                }

                self.child = Some(child);
                self.started_at = Some(Instant::now());
            },
            Err(err) => {
                let logs = self.logs.clone();
                logs.lock().unwrap().push(format!("Failed to start: {:?}", err.to_string()));
                self.status = Some(ExitStatus::from_raw(1));
            }
        }
    }
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {

            let _ = child.kill();

            if let Some(port) = self.port {
                println!("{:?}", pid_from_port(port));
                if let Some(pid) = pid_from_port(port) {
                    println!("{}", pid);
                    println!("{}", port);

                    std::process::Command::new("kill")
                        .args(["-15", &pid])
                        .output()
                        .ok();

                    std::thread::sleep(std::time::Duration::from_millis(500));

                    std::process::Command::new("kill")
                        .args(["-9", &pid])
                        .output()
                        .ok();
                }
            }


            self.started_at = None;
        }
    }

    pub fn restart(&mut self) {
        self.stop();
        self.start();
    }

    pub fn status(&mut self) -> &'static str {
        if let Some(child) = &mut self.child {
            if let Ok(Some(status)) = child.try_wait() {
                self.status = Some(status);
                self.child = None;
                self.started_at = None;
                return "Stopped";
            }
            "Running"
        } else {
            "Stopped"
        }
    }
}

fn pid_from_port(port: u16) -> Option<String> {
    let cmd = format!(
        "ss -lptn | grep :{} | sed -n 's/.*pid=\\([0-9]*\\).*/\\1/p'",
        port
    );

    let output = std::process::Command::new("sh")
        .args(["-c", &cmd])
        .output()
        .ok()?;

    let pid = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string();

    if pid.is_empty() {
        None
    } else {
        Some(pid)
    }
}