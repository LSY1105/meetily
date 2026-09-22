//! ASR sidecar (Python `qmeetily-sidecar-asr`) lifecycle.
//!
//! Spawns the sidecar subprocess on app start, polls `/health` until it
//! answers, and kills it on drop. v0.1 keeps this a tiny child-and-url
//! pair: the ASR sidecar is a long-running HTTP server with no idle-
//! shutdown semantics, so it is simpler than the LLM
//! `summary_engine::SidecarManager`.

use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use parking_lot::Mutex;

const DEFAULT_PORT: u16 = 11436;
const READY_TIMEOUT: Duration = Duration::from_secs(60);

pub struct AsrSidecar {
    child: Mutex<Option<Child>>,
    url: String,
}

impl AsrSidecar {
    pub fn start() -> Result<Arc<Self>> {
        let port: u16 = std::env::var("QMEETILY_ASR_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_PORT);
        let url = format!("http://127.0.0.1:{port}");
        let python = std::env::var("QMEETILY_ASR_PYTHON").unwrap_or_else(|_| "python".into());
        let sidecar_dir = std::env::var("QMEETILY_ASR_DIR").unwrap_or_else(|_| {
            format!("{}/../../sidecar", env!("CARGO_MANIFEST_DIR"))
        });

        let child = Command::new(&python)
            .args(["-m", "qmeetily_sidecar_asr.server"])
            .env("QMEETILY_ASR_PORT", port.to_string())
            .env("HF_ENDPOINT", "https://hf-mirror.com")
            .current_dir(&sidecar_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| {
                format!(
                    "spawn ASR sidecar via `{python} -m qmeetily_sidecar_asr.server` \
                     in {sidecar_dir}"
                )
            })?;

        Ok(Arc::new(Self {
            child: Mutex::new(Some(child)),
            url,
        }))
    }

    pub async fn wait_ready(&self, timeout: Duration) -> Result<()> {
        let client = reqwest::Client::new();
        let health = format!("{}/health", self.url);
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(resp) = client.get(&health).send().await {
                if resp.status().is_success() {
                    return Ok(());
                }
            }
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "ASR sidecar at {} did not become ready within {:?}",
                    self.url,
                    timeout
                );
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// `true` while the child process is still alive (handle present and
    /// `try_wait` returns `Ok(None)`). `false` if the process exited,
    /// `stop()` was called, or the handle was never installed.
    pub fn is_running(&self) -> bool {
        let mut guard = self.child.lock();
        match guard.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    pub fn stop(&self) {
        if let Some(mut c) = self.child.lock().take() {
            let _ = c.kill();
        }
    }
}

impl Drop for AsrSidecar {
    fn drop(&mut self) {
        self.stop();
    }
}
