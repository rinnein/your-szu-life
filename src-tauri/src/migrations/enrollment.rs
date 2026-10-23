use toasty::migration::{MigrationFile, MigrationSet};

pub static ENROLLMENT_MIGRATIONS: MigrationSet = MigrationSet::new(&[MigrationFile::new(
    1,
    "0001_init_enrollment.sql",
    "CREATE TABLE IF NOT EXISTS enrollment_snapshots (
        id TEXT PRIMARY KEY,
        account TEXT NOT NULL,
        kind TEXT NOT NULL,
        scope TEXT NOT NULL,
        payload TEXT NOT NULL,
        updated INTEGER NOT NULL
    );
-- #[toasty::breakpoint]
    CREATE INDEX IF NOT EXISTS idx_enrollment_snapshots_account ON enrollment_snapshots(account);",
)]);
