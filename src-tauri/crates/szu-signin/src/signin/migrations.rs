use toasty::migration::{MigrationFile, MigrationSet};

pub static SIGNIN_MIGRATIONS: MigrationSet = MigrationSet::new(&[
    MigrationFile::new(
        1,
        "0001_init_signin.sql",
        "CREATE TABLE IF NOT EXISTS timetable_cache_records (
            id TEXT PRIMARY KEY,
            payload_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
-- #[toasty::breakpoint]
        CREATE TABLE IF NOT EXISTS rollcall_memory_records (
            course_key TEXT PRIMARY KEY,
            last_point_json TEXT NOT NULL,
            last_photo_b64 TEXT NOT NULL,
            last_photo_mime TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );",
    ),
]);
