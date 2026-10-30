use toasty::migration::{MigrationFile, MigrationSet};

pub static LMS_MIGRATIONS: MigrationSet = MigrationSet::new(&[MigrationFile::new(
    1,
    "0001_init_lms.sql",
    "CREATE TABLE IF NOT EXISTS lms_state_records (
        id TEXT PRIMARY KEY,
        payload_json TEXT NOT NULL,
        updated_at_ms INTEGER NOT NULL
    );",
)]);
