use toasty::migration::{MigrationFile, MigrationSet};

/// 设备同步的本地状态：只存版本向量、内容哈希与冲突标记，不存任何同步值。
pub static MESH_MIGRATIONS: MigrationSet = MigrationSet::new(&[MigrationFile::new(
    1,
    "0001_init_sync_state.sql",
    "CREATE TABLE IF NOT EXISTS sync_items (
        item_id TEXT PRIMARY KEY,
        vector_json TEXT NOT NULL,
        content_hash TEXT NOT NULL,
        conflict INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS sync_rows (
        row_key TEXT PRIMARY KEY,
        content_hash TEXT NOT NULL,
        conflict INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    );",
)]);
