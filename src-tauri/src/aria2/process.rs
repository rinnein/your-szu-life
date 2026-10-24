use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::OnceLock,
    time::Duration,
};
use tokio::sync::Mutex;

pub const DEFAULT_ARIA2_PORT: u16 = 54001;
pub const MAX_ARIA2_PORT_SCAN: u16 = 54100;

fn find_available_aria2_port() -> Result<u16, String> {
    for port in DEFAULT_ARIA2_PORT..=MAX_ARIA2_PORT_SCAN {
        let addr = format!("127.0.0.1:{port}");
        if let Ok(listener) = TcpListener::bind(&addr) {
            drop(listener);
            return Ok(port);
        }
    }
    Err(format!(
        "无法在高位端口范围 {DEFAULT_ARIA2_PORT}..={MAX_ARIA2_PORT_SCAN} 找到可用端口"
    ))
}

pub struct Aria2Process {
    pub port: u16,
    pub secret: String,
    pub log_path: PathBuf,
    child: Option<Child>,
}

impl Aria2Process {
    pub fn spawn(binary_path: &Path) -> Result<Self, String> {
        let port = find_available_aria2_port()?;
        let secret = uuid::Uuid::new_v4().to_string();
        let log_path = std::env::temp_dir().join(format!("aria2c-{port}.log"));

        let mut cmd = Command::new(binary_path);
        cmd.env_remove("ftp_proxy")
            .env_remove("FTP_PROXY")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .arg("--enable-rpc=true")
            .arg("--rpc-listen-all=false")
            .arg(format!("--rpc-listen-port={port}"))
            .arg(format!("--rpc-secret={secret}"))
            .arg("--max-connection-per-server=16")
            .arg("--split=16")
            .arg("--min-split-size=1M")
            .arg("--no-conf=true")
            .arg("--console-log-level=warn")
            .arg(format!("--log={}", log_path.display()))
            .arg("--log-level=warn")
            .arg("--quiet=true")
            .arg("--allow-overwrite=true")
            .arg("--auto-file-renaming=false")
            .arg("--continue=true")
            .arg("--connect-timeout=15")
            .arg("--timeout=15")
            .arg("--max-tries=5")
            .arg("--retry-wait=2")
            .arg("--check-certificate=false");

        // Android 上没有 /etc/resolv.conf，c-ares 异步 DNS 会持续超时导致解析失败或崩溃，
        // 显式禁用 async-dns，回退至 Android 系统的标准 libc getaddrinfo 进行 DNS 解析。
        #[cfg(target_os = "android")]
        {
            cmd.arg("--async-dns=false");
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let child = cmd
            .spawn()
            .map_err(|e| format!("启动 aria2c 失败 (路径: {}): {e}", binary_path.display()))?;

        // Give process a small moment to start listening
        std::thread::sleep(Duration::from_millis(80));

        Ok(Self {
            port,
            secret,
            log_path,
            child: Some(child),
        })
    }

    pub fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}/jsonrpc", self.port)
    }

    pub fn check_alive(&mut self) -> Result<(), String> {
        if let Some(child) = self.child.as_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                let log_tail = self.read_log_tail();
                let detail = if log_tail.is_empty() {
                    String::new()
                } else {
                    format!("，日志内容:\n{log_tail}")
                };
                return Err(format!("aria2c 进程已意外退出 (状态: {status}){detail}"));
            }
        }
        Ok(())
    }

    fn read_log_tail(&self) -> String {
        if let Ok(content) = fs::read_to_string(&self.log_path) {
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(10);
            lines[start..].join("\n")
        } else {
            String::new()
        }
    }

    pub fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_file(&self.log_path);
    }
}

impl Drop for Aria2Process {
    fn drop(&mut self) {
        self.kill();
    }
}

struct Aria2State {
    process: Aria2Process,
    ref_count: usize,
}

static ARIA2_MANAGER: OnceLock<Mutex<Option<Aria2State>>> = OnceLock::new();

fn aria2_manager_mutex() -> &'static Mutex<Option<Aria2State>> {
    ARIA2_MANAGER.get_or_init(|| Mutex::new(None))
}

pub struct Aria2Lease {
    pub port: u16,
    pub ws_url: String,
    pub secret: String,
    released: bool,
}

impl Aria2Lease {
    pub async fn acquire(app: Option<&tauri::AppHandle>) -> Result<Self, String> {
        let mutex = aria2_manager_mutex();
        let mut guard = mutex.lock().await;

        if let Some(state) = guard.as_mut() {
            if state.process.check_alive().is_ok() {
                state.ref_count += 1;
                return Ok(Self {
                    port: state.process.port,
                    ws_url: state.process.ws_url(),
                    secret: state.process.secret.clone(),
                    released: false,
                });
            }
            log::warn!("检测到旧的 aria2c 实例已退出，正在重新启动...");
            if let Some(mut old) = guard.take() {
                old.process.kill();
            }
        }

        let binary_path = crate::aria2::binary::find_aria2c_binary(app)?;
        let process = Aria2Process::spawn(&binary_path)?;
        log::debug!("aria2c 实例已启动，监听端口: {}", process.port);

        let port = process.port;
        let ws_url = process.ws_url();
        let secret = process.secret.clone();

        *guard = Some(Aria2State {
            process,
            ref_count: 1,
        });

        Ok(Self {
            port,
            ws_url,
            secret,
            released: false,
        })
    }

    pub async fn check_alive(&self) -> Result<(), String> {
        let mutex = aria2_manager_mutex();
        let mut guard = mutex.lock().await;
        if let Some(state) = guard.as_mut() {
            state.process.check_alive()?;
        }
        Ok(())
    }

    pub async fn release(mut self) {
        if self.released {
            return;
        }
        self.released = true;

        let mutex = aria2_manager_mutex();
        let mut guard = mutex.lock().await;
        if let Some(state) = guard.as_mut() {
            state.ref_count = state.ref_count.saturating_sub(1);
            if state.ref_count == 0 {
                if let Some(mut old) = guard.take() {
                    log::debug!(
                        "所有 aria2c 下载任务已完成，正在关闭 aria2c 进程 (端口: {})...",
                        old.process.port
                    );
                    old.process.kill();
                }
            }
        }
    }
}

impl Drop for Aria2Lease {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        self.released = true;

        if let Some(mutex) = ARIA2_MANAGER.get() {
            if let Ok(mut guard) = mutex.try_lock() {
                if let Some(state) = guard.as_mut() {
                    state.ref_count = state.ref_count.saturating_sub(1);
                    if state.ref_count == 0 {
                        if let Some(mut old) = guard.take() {
                            log::debug!(
                                "所有 aria2c 下载任务已结束，正在关闭 aria2c 进程 (端口: {})...",
                                old.process.port
                            );
                            old.process.kill();
                        }
                    }
                }
            } else {
                tokio::spawn(async move {
                    let mutex = aria2_manager_mutex();
                    let mut guard = mutex.lock().await;
                    if let Some(state) = guard.as_mut() {
                        state.ref_count = state.ref_count.saturating_sub(1);
                        if state.ref_count == 0 {
                            if let Some(mut old) = guard.take() {
                                log::debug!(
                                    "所有 aria2c 下载任务已结束，正在关闭 aria2c 进程 (端口: {})...",
                                    old.process.port
                                );
                                old.process.kill();
                            }
                        }
                    }
                });
            }
        }
    }
}
