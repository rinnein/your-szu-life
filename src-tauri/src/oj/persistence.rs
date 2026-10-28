use super::model::Data;
use std::path::Path;

pub(super) fn read(path: &Path) -> Result<Data, String> {
    // 1. 如果路径直接存在且为 json 文件（用于兼容历史格式测试），尝试解析
    if path.extension().is_some_and(|ext| ext == "json") && path.exists() {
        if let Ok(bytes) = std::fs::read(path) {
            if bytes.len() > 32 * 1024 * 1024 {
                return Err("OJ 本地数据过大".into());
            }
            if let Ok(data) = serde_json::from_slice::<Data>(&bytes) {
                if data.version == 1 {
                    return Ok(data);
                }
                return Err("OJ 本地数据版本不兼容".into());
            }
        }
    }

    // 2. 检查对应的 sqlite 数据库路径
    let db_path = if path.extension().is_some_and(|ext| ext == "sqlite") {
        path.to_path_buf()
    } else {
        path.with_extension("sqlite")
    };

    let res = crate::migrations::db::run_sync(async {
        let db = super::db::OjDatabase::open(&db_path).await?;
        if let Ok(Some(migrated)) = crate::migrations::oj::migrate_oj_state_if_needed(&db_path, &db).await {
            return Ok(migrated);
        }
        db.load_data().await
    });

    // 首次使用时文件会被 open_sqlite_db 创建，load_data 返回 Data::default()。
    // 其它情况下读取失败必须上抛，否则默认数据会在下一次写入时覆盖真实状态。
    res.map_err(|e| format!("无法读取 OJ 本地数据: {e}"))
}

pub(super) fn write(path: &Path, data: &Data) -> Result<(), String> {
    let db_path = if path.extension().is_some_and(|ext| ext == "sqlite") {
        path.to_path_buf()
    } else {
        path.with_extension("sqlite")
    };

    crate::migrations::oj::cleanup_legacy_oj_files(&db_path);

    crate::migrations::db::run_sync(async {
        let db = super::db::OjDatabase::open(&db_path).await?;
        db.save_data(data).await
    })
}
