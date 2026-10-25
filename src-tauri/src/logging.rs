use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::SystemTime,
};
use tauri::{AppHandle, Manager};

pub(crate) const APP_LOG_PREFIX: &str = "your-szu-life";
const MAX_STARTUP_BUFFER_BYTES: usize = 256 * 1024;
const MAX_LOG_SIZE_BYTES: u64 = 10 * 1024 * 1024; // 10MB
const MAX_RETENTION_DAYS: i64 = 7;
const CONFIG_FILE_NAME: &str = "log-settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogFileInfo {
    pub(crate) name: String,
    pub(crate) size_bytes: u64,
    pub(crate) modified_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogInfo {
    pub(crate) today_log_path: String,
    pub(crate) log_dir: String,
    pub(crate) total_size_bytes: u64,
    pub(crate) file_count: usize,
    pub(crate) current_level: String,
    pub(crate) available_levels: Vec<String>,
    pub(crate) files: Vec<LogFileInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogConfig {
    level: String,
}

pub(crate) struct LogState {
    pub(crate) log_dir: OnceLock<PathBuf>,
    pub(crate) config_dir: OnceLock<PathBuf>,
    inner: Mutex<WriterInner>,
    active_level: AtomicU8,
}

struct WriterInner {
    current_date: Option<String>,
    current_file: Option<File>,
    startup_buffer: Vec<u8>,
}

pub(crate) struct DailyLogWriter {
    state: Arc<LogState>,
}

impl Write for DailyLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self
            .state
            .inner
            .lock()
            .map_err(|e| io::Error::other(e.to_string()))?;

        if let Some(log_dir) = self.state.log_dir.get() {
            let today = today_date_str();
            let need_new_file =
                inner.current_date.as_deref() != Some(&today) || inner.current_file.is_none();

            if need_new_file {
                inner.current_file = None;
                if !log_dir.exists() {
                    let _ = fs::create_dir_all(log_dir);
                }
                let file_path = log_dir.join(format!("{APP_LOG_PREFIX}_{today}.log"));
                match OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&file_path)
                {
                    Ok(mut file) => {
                        if !inner.startup_buffer.is_empty() {
                            let _ = file.write_all(&inner.startup_buffer);
                            inner.startup_buffer.clear();
                        }
                        inner.current_file = Some(file);
                        inner.current_date = Some(today);
                        let _ = auto_cleanup(log_dir);
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }

            if let Some(file) = &mut inner.current_file {
                file.write_all(buf)?;
            }
        } else if inner.startup_buffer.len() < MAX_STARTUP_BUFFER_BYTES {
            inner.startup_buffer.extend_from_slice(buf);
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self
            .state
            .inner
            .lock()
            .map_err(|e| io::Error::other(e.to_string()))?;
        if let Some(file) = &mut inner.current_file {
            file.flush()?;
        }
        Ok(())
    }
}

static GLOBAL_LOG_STATE: OnceLock<Arc<LogState>> = OnceLock::new();

pub(crate) fn get_log_state() -> Arc<LogState> {
    GLOBAL_LOG_STATE
        .get_or_init(|| {
            Arc::new(LogState {
                log_dir: OnceLock::new(),
                config_dir: OnceLock::new(),
                inner: Mutex::new(WriterInner {
                    current_date: None,
                    current_file: None,
                    startup_buffer: Vec::new(),
                }),
                active_level: AtomicU8::new(log::LevelFilter::Info as usize as u8),
            })
        })
        .clone()
}

pub(crate) fn create_daily_writer() -> (
    DailyLogWriter,
    impl Fn(&log::Metadata) -> bool + Send + Sync + 'static,
) {
    let state = get_log_state();
    let writer_state = state.clone();
    let filter_state = state.clone();

    let writer = DailyLogWriter {
        state: writer_state,
    };
    let filter = move |metadata: &log::Metadata| {
        let max_val = filter_state.active_level.load(Ordering::Relaxed);
        let max_level = level_from_u8(max_val);
        metadata.level() <= max_level
    };

    (writer, filter)
}

pub(crate) fn initialize(app: &AppHandle) {
    let state = get_log_state();

    if let Ok(dir) = app.path().app_log_dir() {
        let _ = fs::create_dir_all(&dir);
        let _ = state.log_dir.set(dir.clone());

        // Drain any startup buffer into today's log file
        if let Ok(mut inner) = state.inner.lock() {
            let today = today_date_str();
            let file_path = dir.join(format!("{APP_LOG_PREFIX}_{today}.log"));
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&file_path)
            {
                if !inner.startup_buffer.is_empty() {
                    let _ = file.write_all(&inner.startup_buffer);
                    inner.startup_buffer.clear();
                }
                inner.current_file = Some(file);
                inner.current_date = Some(today);
            }
        }

        let _ = auto_cleanup(&dir);
    }

    if let Ok(config_dir) = app.path().app_config_dir() {
        let _ = state.config_dir.set(config_dir.clone());
        let config_file = config_dir.join(CONFIG_FILE_NAME);
        if let Ok(content) = fs::read_to_string(&config_file) {
            if let Ok(config) = serde_json::from_str::<LogConfig>(&content) {
                if let Some(level) = parse_level(&config.level) {
                    state
                        .active_level
                        .store(level as usize as u8, Ordering::Release);
                }
            }
        }
    }
}

pub(crate) fn today_date_str() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

pub(crate) fn timestamp_now() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

fn level_from_u8(val: u8) -> log::LevelFilter {
    match val {
        1 => log::LevelFilter::Error,
        2 => log::LevelFilter::Warn,
        3 => log::LevelFilter::Info,
        4 => log::LevelFilter::Debug,
        5 => log::LevelFilter::Trace,
        _ => log::LevelFilter::Info,
    }
}

fn parse_level(str_level: &str) -> Option<log::LevelFilter> {
    match str_level.to_uppercase().as_str() {
        "ERROR" => Some(log::LevelFilter::Error),
        "WARN" | "WARNING" => Some(log::LevelFilter::Warn),
        "INFO" => Some(log::LevelFilter::Info),
        "DEBUG" => Some(log::LevelFilter::Debug),
        "TRACE" => Some(log::LevelFilter::Trace),
        _ => None,
    }
}

fn level_to_str(filter: log::LevelFilter) -> &'static str {
    match filter {
        log::LevelFilter::Error => "ERROR",
        log::LevelFilter::Warn => "WARN",
        log::LevelFilter::Info => "INFO",
        log::LevelFilter::Debug => "DEBUG",
        log::LevelFilter::Trace => "TRACE",
        log::LevelFilter::Off => "OFF",
    }
}

pub(crate) fn set_log_level(level_str: &str) -> Result<String, String> {
    let state = get_log_state();
    let filter = parse_level(level_str)
        .ok_or_else(|| format!("无效的日志级别: {level_str}，支持 DEBUG, INFO, WARN, ERROR"))?;

    state
        .active_level
        .store(filter as usize as u8, Ordering::Release);

    if let Some(config_dir) = state.config_dir.get() {
        let _ = fs::create_dir_all(config_dir);
        let config_file = config_dir.join(CONFIG_FILE_NAME);
        let config = LogConfig {
            level: level_to_str(filter).to_string(),
        };
        if let Ok(raw) = serde_json::to_string_pretty(&config) {
            let _ = fs::write(config_file, raw);
        }
    }

    log::info!("日志文件保留级别已设置为: {}", level_to_str(filter));
    Ok(level_to_str(filter).to_string())
}

pub(crate) fn get_log_info(app: &AppHandle) -> Result<LogInfo, String> {
    let state = get_log_state();
    let log_dir = match state.log_dir.get() {
        Some(dir) => dir.clone(),
        None => app.path().app_log_dir().map_err(|e| e.to_string())?,
    };

    let today = today_date_str();
    let today_log_path = log_dir.join(format!("{APP_LOG_PREFIX}_{today}.log"));

    let mut total_size_bytes = 0u64;
    let mut files = Vec::new();

    if let Ok(entries) = fs::read_dir(&log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "log" {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        let modified_ms = entry
                            .metadata()
                            .and_then(|m| m.modified())
                            .ok()
                            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);

                        total_size_bytes += size;
                        files.push(LogFileInfo {
                            name,
                            size_bytes: size,
                            modified_ms,
                        });
                    }
                }
            }
        }
    }

    // Sort files descending by modified time (newest first)
    files.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));

    let current_level =
        level_to_str(level_from_u8(state.active_level.load(Ordering::Acquire))).to_string();

    Ok(LogInfo {
        today_log_path: today_log_path.to_string_lossy().to_string(),
        log_dir: log_dir.to_string_lossy().to_string(),
        total_size_bytes,
        file_count: files.len(),
        current_level,
        available_levels: vec![
            "DEBUG".to_string(),
            "INFO".to_string(),
            "WARN".to_string(),
            "ERROR".to_string(),
        ],
        files,
    })
}

pub(crate) fn clear_logs(app: &AppHandle) -> Result<LogInfo, String> {
    let state = get_log_state();
    let log_dir = match state.log_dir.get() {
        Some(dir) => dir.clone(),
        None => app.path().app_log_dir().map_err(|e| e.to_string())?,
    };

    if let Ok(mut inner) = state.inner.lock() {
        inner.current_file = None;
        inner.current_date = None;
        inner.startup_buffer.clear();
    }

    if let Ok(entries) = fs::read_dir(&log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "log" || ext == "jsonl" || ext == "bak" {
                        let _ = fs::remove_file(path);
                    }
                }
            }
        }
    }

    log::info!("所有本地日志已清空");
    get_log_info(app)
}

pub(crate) fn auto_cleanup(log_dir: &Path) -> io::Result<()> {
    if !log_dir.exists() {
        return Ok(());
    }

    let today = Local::now().date_naive();
    let entries = fs::read_dir(log_dir)?;

    struct FileEntry {
        path: PathBuf,
        date: Option<NaiveDate>,
        modified: SystemTime,
        size: u64,
    }

    let mut log_files = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let is_log = path.extension().is_some_and(|ext| ext == "log");
        if !is_log {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        // Parse date from file name: your-szu-life_YYYY-MM-DD.log
        let date = parse_date_from_file_name(file_name);
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let size = metadata.len();

        log_files.push(FileEntry {
            path,
            date,
            modified,
            size,
        });
    }

    // 1. Time-based retention: remove files older than 7 days
    log_files.retain(|file| {
        let is_expired = if let Some(file_date) = file.date {
            (today - file_date).num_days() >= MAX_RETENTION_DAYS
        } else if let Ok(elapsed) = SystemTime::now().duration_since(file.modified) {
            elapsed.as_secs() >= (MAX_RETENTION_DAYS as u64) * 86400
        } else {
            false
        };

        if is_expired {
            let _ = fs::remove_file(&file.path);
            false
        } else {
            true
        }
    });

    // 2. Size-based retention: if total size >= 10MB, delete oldest files until under 10MB
    let total_size: u64 = log_files.iter().map(|f| f.size).sum();
    if total_size >= MAX_LOG_SIZE_BYTES {
        // Sort oldest first: by date if both have it, otherwise by modified time
        log_files.sort_by(|a, b| match (a.date, b.date) {
            (Some(da), Some(db)) => da.cmp(&db),
            _ => a.modified.cmp(&b.modified),
        });

        let mut current_total = total_size;
        for file in log_files {
            if current_total < MAX_LOG_SIZE_BYTES {
                break;
            }
            if fs::remove_file(&file.path).is_ok() {
                current_total = current_total.saturating_sub(file.size);
            }
        }
    }

    Ok(())
}

fn parse_date_from_file_name(file_name: &str) -> Option<NaiveDate> {
    let name_without_ext = file_name.strip_suffix(".log")?;
    let date_part = name_without_ext
        .strip_prefix(&format!("{APP_LOG_PREFIX}_"))
        .unwrap_or(name_without_ext);

    // Matches YYYY-MM-DD
    if date_part.len() == 10 {
        NaiveDate::parse_from_str(date_part, "%Y-%m-%d").ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_date_from_file_names() {
        assert_eq!(
            parse_date_from_file_name("your-szu-life_2026-10-25.log"),
            Some(NaiveDate::from_ymd_opt(2026, 10, 25).unwrap())
        );
        assert_eq!(
            parse_date_from_file_name("2026-10-25.log"),
            Some(NaiveDate::from_ymd_opt(2026, 10, 25).unwrap())
        );
        assert_eq!(parse_date_from_file_name("invalid.log"), None);
    }

    #[test]
    fn auto_cleanup_deletes_old_files() {
        let dir = tempdir().unwrap();
        let log_dir = dir.path();

        let old_date = Local::now().date_naive() - chrono::Duration::days(8);
        let old_file = log_dir.join(format!(
            "{APP_LOG_PREFIX}_{}.log",
            old_date.format("%Y-%m-%d")
        ));
        let today = Local::now().date_naive();
        let today_file = log_dir.join(format!("{APP_LOG_PREFIX}_{}.log", today.format("%Y-%m-%d")));

        fs::write(&old_file, "old log content").unwrap();
        fs::write(&today_file, "today log content").unwrap();

        assert!(old_file.exists());
        assert!(today_file.exists());

        auto_cleanup(log_dir).unwrap();

        assert!(!old_file.exists(), "Files >= 7 days should be deleted");
        assert!(today_file.exists(), "Today's file should be retained");
    }

    #[test]
    fn auto_cleanup_deletes_excess_size_oldest_first() {
        let dir = tempdir().unwrap();
        let log_dir = dir.path();

        let date1 = Local::now().date_naive() - chrono::Duration::days(3);
        let date2 = Local::now().date_naive() - chrono::Duration::days(1);
        let date3 = Local::now().date_naive();

        let file1 = log_dir.join(format!("{APP_LOG_PREFIX}_{}.log", date1.format("%Y-%m-%d")));
        let file2 = log_dir.join(format!("{APP_LOG_PREFIX}_{}.log", date2.format("%Y-%m-%d")));
        let file3 = log_dir.join(format!("{APP_LOG_PREFIX}_{}.log", date3.format("%Y-%m-%d")));

        // Create 3 files: 6MB, 4MB, 4MB (total 14MB > 10MB limit)
        // Deleting file1 (6MB) leaves 8MB (< 10MB limit), so file2 and file3 are kept.
        let chunk6 = vec![b'x'; 6 * 1024 * 1024];
        let chunk4 = vec![b'x'; 4 * 1024 * 1024];
        fs::write(&file1, &chunk6).unwrap();
        fs::write(&file2, &chunk4).unwrap();
        fs::write(&file3, &chunk4).unwrap();

        auto_cleanup(log_dir).unwrap();

        // Oldest file should be removed first to get under 10MB
        assert!(!file1.exists(), "Oldest file should be deleted");
        assert!(file2.exists());
        assert!(file3.exists());
    }
}
