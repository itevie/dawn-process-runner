use std::{
    collections::VecDeque,
    io::{BufRead, BufReader},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const MAX_LOG_LINES: usize = 2000;
const GRACEFUL_TIMEOUT: Duration = Duration::from_millis(1000);

pub struct ManagedProcess {
    pub name: String,
    pub command: Vec<String>,
    pub cwd: Option<String>,
    pub port: Option<u16>,

    pub child: Option<Child>,
    pub logs: Arc<Mutex<Vec<String>>>,
    pub started_at: Option<Instant>,
    pub exit_status: Option<ExitStatus>,

    special_status: Option<String>,
}

impl ManagedProcess {
    pub fn new(
        name: &str,
        command: Vec<String>,
        cwd: Option<String>,
        port: Option<u16>,
    ) -> Self {
        Self {
            name: name.to_string(),
            command,
            cwd,
            port,
            child: None,
            logs: Arc::new(Mutex::new(Vec::new())),
            started_at: None,
            exit_status: None,
            special_status: None
        }
    }

    pub fn start(&mut self) {
        if self.child.is_some() {
            return;
        }

        if self.command.is_empty() {
            self.push_log("Command is empty");
            return;
        }

        let mut cmd = Command::new(&self.command[0]);
        cmd.args(&self.command[1..]);

        if let Some(ref cwd) = self.cwd {
            cmd.current_dir(cwd);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        match cmd.spawn() {
            Ok(mut child) => {
                self.started_at = Some(Instant::now());
                self.exit_status = None;

                self.push_log(format!("Started: {}", self.command.join(" ")));

                self.spawn_reader(child.stdout.take());
                self.spawn_reader(child.stderr.take());

                self.child = Some(child);
            }
            Err(e) => {
                self.push_log(format!("Failed to start: {}", e));
            }
        }
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let pid = child.id().to_string();

            // --- Graceful shutdown ---
            self.special_status = Some("Killing".to_string());
            let _ = Command::new("kill").args(["-15", &pid]).output();

            let start = Instant::now();

            let mut success = false;
            while start.elapsed() < GRACEFUL_TIMEOUT {
                if let Ok(Some(status)) = child.try_wait() {
                    self.exit_status = Some(status);
                    self.started_at = None;
                    self.push_log("Stopped gracefully");
                    self.special_status = Some("Killed Gracefully".to_string());
                    success = true;
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }

            // --- Force kill ---
            if !success {
                let _ = Command::new("kill").args(["-9", &pid]).output();
                let _ = child.wait();
                self.special_status = Some("Force Killed".to_string());

                self.push_log("Force killed");
            }
        }

        // Optional fallback: kill by port
        if let Some(port) = self.port {
            self.special_status = Some("Killing By Port".to_string());
            if let Some(pid) = pid_from_port(port) {
                let _ = Command::new("kill").args(["-9", &pid]).output();
                self.push_log(format!("Killed PID {} on port {}", pid, port));
                self.special_status = Some(format!("Killed {}", pid));
            }
        }

        self.started_at = None;
    }

    pub fn restart(&mut self) {
        self.stop();
        self.start();
    }

    pub fn status(&mut self) -> String {
        if let Some(ref special) = self.special_status {
            return special.clone();
        }

        if let Some(child) = &mut self.child {
            if let Ok(Some(status)) = child.try_wait() {
                self.exit_status = Some(status);
                self.child = None;
                self.started_at = None;
                return "Stopped".to_string();
            }
            return "Running".to_string();
        }
        "Stopped".to_string()
    }

    pub fn logs(&self) -> Vec<String> {
        self.logs
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect()
    }

    fn spawn_reader(&self, stream: Option<impl std::io::Read + Send + 'static>) {
        if let Some(stream) = stream {
            let logs = self.logs.clone();
            thread::spawn(move || {
                let reader = BufReader::new(stream);
                for line in reader.lines().flatten() {
                    let mut guard = logs.lock().unwrap();
                    guard.push(line);
                }
            });
        }
    }

    fn push_log<S: Into<String>>(&self, msg: S) {
        let v = msg.into();
        let mut logs = self.logs.lock().unwrap();
        logs.push(v);
    }
}

fn pid_from_port(port: u16) -> Option<String> {
    let output = Command::new("ss")
        .args(["-lptn"])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        if line.contains(&format!(":{}", port)) {
            if let Some(start) = line.find("pid=") {
                let pid_part = &line[start + 4..];
                let pid: String = pid_part
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if !pid.is_empty() {
                    return Some(pid);
                }
            }
        }
    }

    None
}