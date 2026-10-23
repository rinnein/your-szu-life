use std::{
    net::TcpListener,
    path::Path,
    process::{Child, Command},
    time::Duration,
};

pub struct Aria2Process {
    pub port: u16,
    pub secret: String,
    child: Option<Child>,
}

impl Aria2Process {
    pub fn spawn(binary_path: &Path) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| format!("分配本地 RPC 端口失败: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("获取本地端口失败: {e}"))?
            .port();
        drop(listener);

        let secret = uuid::Uuid::new_v4().to_string();

        let mut cmd = Command::new(binary_path);
        cmd.env_remove("ftp_proxy")
            .env_remove("FTP_PROXY")
            .arg("--enable-rpc=true")
            .arg("--rpc-listen-all=false")
            .arg(format!("--rpc-listen-port={port}"))
            .arg(format!("--rpc-secret={secret}"))
            .arg("--max-connection-per-server=16")
            .arg("--split=16")
            .arg("--min-split-size=1M")
            .arg("--no-conf=true")
            .arg("--console-log-level=warn")
            .arg("--quiet=true")
            .arg("--allow-overwrite=true")
            .arg("--auto-file-renaming=false");

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
            child: Some(child),
        })
    }

    pub fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}/jsonrpc", self.port)
    }

    pub fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Aria2Process {
    fn drop(&mut self) {
        self.kill();
    }
}
