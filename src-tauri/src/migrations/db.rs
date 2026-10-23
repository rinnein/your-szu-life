use std::path::Path;
use toasty::migration::MigrationSet;
use toasty::ModelSet;

/// 统一的 SQLite 数据库打开与迁移管理助手。
///
/// 1. 自动确保父目录存在；
/// 2. 具备损坏文件自愈能力（如遇 "not a database" 异常，安全重置重建）；
/// 3. 使用 Toasty 原生 MigrationSet 应用数据库迁移版本，自动通过 __toasty_migrations 表跟踪状态，
///    避免手写 SQL 字符串匹配或 "table already exists" hack。
pub async fn open_sqlite_db(
    db_path: &Path,
    models: impl Into<ModelSet>,
    migrations: Option<&MigrationSet>,
) -> Result<toasty::Db, String> {
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let url = format!("sqlite://{}", db_path.display());
    let models_data = models.into();

    let db = match toasty::Db::builder()
        .models(models_data.clone())
        .connect(&url)
        .await
    {
        Ok(db) => db,
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("not a database") {
                let _ = std::fs::remove_file(db_path);
                return Box::pin(open_sqlite_db(db_path, models_data, migrations)).await;
            }
            return Err(format!(
                "连接 SQLite 数据库失败 ({path}): {e}",
                path = db_path.display()
            ));
        }
    };

    if let Some(m) = migrations {
        if let Err(e) = m.apply(&db).await {
            let err_str = e.to_string();
            if err_str.contains("not a database") {
                let _ = std::fs::remove_file(db_path);
                return Box::pin(open_sqlite_db(db_path, models_data, migrations)).await;
            }
            return Err(format!(
                "执行数据库迁移失败 ({path}): {e}",
                path = db_path.display()
            ));
        }
    } else if let Err(e) = db.push_schema().await {
        let err_str = e.to_string();
        if err_str.contains("not a database") {
            let _ = std::fs::remove_file(db_path);
            return Box::pin(open_sqlite_db(db_path, models_data, migrations)).await;
        }
        if !err_str.contains("already exists") {
            return Err(format!(
                "推送数据库结构失败 ({path}): {e}",
                path = db_path.display()
            ));
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
