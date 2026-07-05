use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;

const MAX_RESTARTS: u32 = 3;
const RPC_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub id: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

struct SidecarInner {
    child: Option<Child>,
    stdin: Option<std::process::ChildStdin>,
    stdout: Option<std::process::ChildStdout>,
}

pub struct SidecarProcess {
    inner: Mutex<SidecarInner>,
    node_path: String,
    sidecar_dir: PathBuf,
    next_id: Mutex<u64>,
    consecutive_restarts: Mutex<u32>,
}

impl SidecarProcess {
    pub fn new(node: &str, sidecar_dir: &PathBuf) -> Self {
        Self {
            inner: Mutex::new(SidecarInner {
                child: None,
                stdin: None,
                stdout: None,
            }),
            node_path: node.to_string(),
            sidecar_dir: sidecar_dir.clone(),
            next_id: Mutex::new(1),
            consecutive_restarts: Mutex::new(0),
        }
    }

    pub fn start(&self) -> Result<(), String> {
        let mut inner = self.inner.lock().map_err(|e| e.to_string())?;
        if inner.child.is_some() {
            return Ok(());
        }

        let mut child = Command::new(&self.node_path)
            .arg("index.js")
            .current_dir(&self.sidecar_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("failed to start sidecar: {}", e))?;

        println!(
            "[Sidecar] started (PID: {}) at {}",
            child.id(),
            self.sidecar_dir.display()
        );

        inner.stdin = child.stdin.take();
        inner.stdout = child.stdout.take();
        inner.child = Some(child);

        Ok(())
    }

    pub fn stop(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            if let Some(ref mut c) = inner.child {
                let _ = c.kill();
                let _ = c.wait();
            }
            inner.child = None;
            inner.stdin = None;
            inner.stdout = None;
        }
        println!("[Sidecar] stopped");
    }

    pub fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let attempt = || -> Result<serde_json::Value, String> {
            // Check process is alive
            {
                let mut inner = self.inner.lock().map_err(|e| e.to_string())?;
                let alive = inner
                    .child
                    .as_mut()
                    .map_or(false, |c| c.try_wait().ok().map_or(false, |s| s.is_none()));
                if !alive {
                    drop(inner);
                    self.stop();
                    self.start()?;
                }
            }
            self.call_inner(method, params.clone())
        };

        match attempt() {
            Ok(val) => {
                if let Ok(mut r) = self.consecutive_restarts.lock() {
                    *r = 0;
                }
                Ok(val)
            }
            Err(e) => {
                let mut r = self
                    .consecutive_restarts
                    .lock()
                    .map_err(|e| e.to_string())?;
                if *r >= MAX_RESTARTS {
                    return Err(format!(
                        "sidecar {} consecutive failures (max {}): {}",
                        *r, MAX_RESTARTS, e
                    ));
                }
                *r += 1;
                let attempt_num = *r;
                drop(r);

                eprintln!(
                    "[Sidecar] error (attempt {}/{}): {} — restarting",
                    attempt_num, MAX_RESTARTS, e
                );
                std::thread::sleep(Duration::from_millis(300));
                self.stop();
                self.start()?;

                self.call_inner(method, params)
            }
        }
    }

    fn call_inner(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let id = {
            let mut n = self.next_id.lock().map_err(|e| e.to_string())?;
            let id = *n;
            *n += 1;
            id
        };

        let req = JsonRpcRequest {
            id,
            method: method.to_string(),
            params,
        };
        let req_json = serde_json::to_string(&req).map_err(|e| e.to_string())?;

        // Take stdin/stdout under lock, write, drop lock before I/O
        let stdout_taken;
        {
            let mut inner = self.inner.lock().map_err(|e| e.to_string())?;
            let stdin = inner.stdin.as_mut().ok_or_else(|| "no stdin".to_string())?;
            writeln!(stdin, "{}", req_json).map_err(|e| format!("write error: {}", e))?;
            stdout_taken = inner.stdout.take();
        }

        let mut child_stdout = stdout_taken.ok_or_else(|| "no stdout".to_string())?;

        // Read in a thread with timeout
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(&mut child_stdout);
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(_) => {
                    let _ = tx.send(Some((line, child_stdout)));
                }
                Err(_) => {
                    let _ = tx.send(None);
                }
            }
        });

        let (line, restored_stdout) = match rx.recv_timeout(RPC_TIMEOUT) {
            Ok(Some((l, s))) => (l, Some(s)),
            Ok(None) => return Err("read error from sidecar".into()),
            Err(_) => return Err(format!("RPC timeout ({}s)", RPC_TIMEOUT.as_secs())),
        };

        // Return stdout handle
        if let Ok(mut inner) = self.inner.lock() {
            inner.stdout = restored_stdout;
        }

        if line.is_empty() {
            return Err("empty response".into());
        }

        let resp: JsonRpcResponse =
            serde_json::from_str(&line).map_err(|e| format!("parse error: {}", e))?;

        if let Some(err) = resp.error {
            return Err(format!("RPC error {}: {}", err.code, err.message));
        }

        Ok(resp.result.unwrap_or(serde_json::Value::Null))
    }
}

impl Drop for SidecarProcess {
    fn drop(&mut self) {
        self.stop();
    }
}
