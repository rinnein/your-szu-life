use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use toasty::migration::MigrationSet;
use toasty::ModelSet;

/// 进程内按文件路径复用的数据库句柄。
///
/// 过去每次读写都会重新 `connect` 并重建一次 schema，既昂贵又会在同一文件上
/// 产生多个连接池，容易触发 `database is locked`。这里保证一个文件只有一份
/// `toasty::Db`，迁移也只在首次打开时执行。
static DATABASES: OnceLock<Mutex<HashMap<PathBuf, toasty::Db>>> = OnceLock::new();

/// 串行化“首次打开”：多个模块可能同时调用，如果两个线程同时执行迁移，
/// `ALTER TABLE ... ADD COLUMN` 会有一方失败。
static OPEN_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn open_lock() -> &'static tokio::sync::Mutex<()> {
    OPEN_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn registry() -> &'static Mutex<HashMap<PathBuf, toasty::Db>> {
    DATABASES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 路径可能来自不同拼写（相对路径、未创建的父目录），统一成可直接比较的键。
fn cache_key(db_path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(db_path) {
        return canonical;
    }
    match (db_path.parent(), db_path.file_name()) {
        (Some(parent), Some(name)) => match std::fs::canonicalize(parent) {
            Ok(parent) => parent.join(name),
            Err(_) => db_path.to_path_buf(),
        },
        _ => db_path.to_path_buf(),
    }
}

fn cached(db_path: &Path) -> Option<toasty::Db> {
    let key = cache_key(db_path);
    let mut databases = registry().lock().unwrap();
    match databases.get(&key) {
        // 文件被外部删除（例如清理缓存）时丢弃旧句柄，避免连接指向已不存在的 inode。
        Some(db) if key.exists() => Some(db.clone()),
        Some(_) => {
            databases.remove(&key);
            None
        }
        None => None,
    }
}

fn remember(db_path: &Path, db: &toasty::Db) {
    registry()
        .lock()
        .unwrap()
        .insert(cache_key(db_path), db.clone());
}

/// 仅供测试：清空句柄缓存，模拟应用重启。
#[cfg(test)]
pub fn reset_registry() {
    registry().lock().unwrap().clear();
}

/// 把损坏的数据库文件连同 WAL/SHM 一起改名隔离，而不是直接删除用户数据。
fn quarantine(db_path: &Path) {
    let stamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    for suffix in ["", "-wal", "-shm"] {
        let source = PathBuf::from(format!("{}{suffix}", db_path.display()));
        if !source.exists() {
            continue;
        }
        let target = PathBuf::from(format!("{}.corrupt-{stamp}{suffix}", db_path.display()));
        if let Err(error) = std::fs::rename(&source, &target) {
            log::warn!("无法隔离损坏的数据库文件 {}: {error}", source.display());
        } else {
            log::warn!(
                "数据库文件损坏，已移动到 {}（原数据保留，可手动恢复）",
                target.display()
            );
        }
    }
}

/// 统一的 SQLite 数据库打开与迁移管理助手。
///
/// 1. 自动确保父目录存在；
/// 2. 同一路径返回同一个连接池，迁移只执行一次；
/// 3. 使用 Toasty 原生 MigrationSet 应用数据库迁移版本，自动通过 __toasty_migrations 表跟踪状态；
/// 4. 遇到损坏文件时改名隔离并重建，绝不静默删除用户数据。
pub async fn open_sqlite_db(
    db_path: &Path,
    models: impl Into<ModelSet>,
    migrations: Option<&MigrationSet>,
) -> Result<toasty::Db, String> {
    if let Some(db) = cached(db_path) {
        return Ok(db);
    }
    // 首次打开（含迁移）串行执行，避免两个线程同时 ALTER TABLE。
    let _opening = open_lock().lock().await;
    if let Some(db) = cached(db_path) {
        return Ok(db);
    }
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let url = format!("sqlite://{}", db_path.display());
    let models_data = models.into();

    // 损坏文件只隔离一次：隔离后重建仍失败则把真实错误交给调用方。
    let mut quarantined = false;
    loop {
        match build(&url, &models_data, migrations).await {
            Ok(db) => {
                remember(db_path, &db);
                return Ok(db);
            }
            Err(message) if message.contains("not a database") && !quarantined => {
                quarantined = true;
                quarantine(db_path);
            }
            Err(message) => {
                return Err(format!(
                    "打开 SQLite 数据库失败 ({path}): {message}",
                    path = db_path.display()
                ));
            }
        }
    }
}

async fn build(
    url: &str,
    models: &ModelSet,
    migrations: Option<&MigrationSet>,
) -> Result<toasty::Db, String> {
    // SQLite 同一时刻只能有一个写者；单连接池可以避免进程内互相抢锁，
    // 也不必依赖 driver 未暴露的 busy_timeout。
    let db = toasty::Db::builder()
        .models(models.clone())
        .max_pool_size(1)
        .connect(url)
        .await
        .map_err(|error| error.to_string())?;

    if let Some(migrations) = migrations {
        migrations
            .apply(&db)
            .await
            .map_err(|error| error.to_string())?;
    } else if let Err(error) = db.push_schema().await {
        let message = error.to_string();
        if !message.contains("already exists") {
            return Err(message);
        }
    }

    Ok(db)
}

/// 统一的跨线程同步执行器。
///
/// 用于在同步业务上下文或测试中安全调用 Toasty 的异步操作，
/// 避免 Tokio 单线程 worker 上的 "cannot start runtime within a runtime" 或 block_in_place panic。
pub fn run_sync<F, T>(fut: F) -> T
where
    F: std::future::Future<Output = T> + Send,
    T: Send,
{
    std::thread::scope(|s| {
        s.spawn(|| tauri::async_runtime::block_on(fut))
            .join()
            .expect("数据库后台任务执行失败")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, toasty::Model)]
    struct ReuseRecord {
        #[key]
        id: String,
        value: String,
    }

    #[tokio::test]
    async fn reuses_handle_until_file_disappears() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reuse.sqlite");
        open_sqlite_db(&path, toasty::models!(ReuseRecord), None)
            .await
            .unwrap();
        assert!(cached(&path).is_some());

        std::fs::remove_file(&path).unwrap();
        assert!(cached(&path).is_none());
    }

    #[tokio::test]
    async fn quarantines_corrupt_file_instead_of_deleting_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.sqlite");
        std::fs::write(&path, b"this is not a sqlite database").unwrap();

        let db = open_sqlite_db(&path, toasty::models!(ReuseRecord), None)
            .await
            .unwrap();
        assert!(path.exists());

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            entries
                .iter()
                .any(|name| name.starts_with("corrupt.sqlite.corrupt-")),
            "损坏文件应被保留为 .corrupt-* 备份: {entries:?}"
        );

        toasty::create!(ReuseRecord {
            id: "a".to_string(),
            value: "b".to_string(),
        })
        .exec(&mut db.clone())
        .await
        .unwrap();
    }
}
