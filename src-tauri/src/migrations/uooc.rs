use crate::uooc::quiz::db::QaDatabase;
use std::collections::HashMap;
use std::path::Path;
use toasty::migration::{MigrationFile, MigrationSet};

pub static UOOC_MIGRATIONS: MigrationSet = MigrationSet::new(&[MigrationFile::new(
    1,
    "0001_init_uooc.sql",
    "CREATE TABLE IF NOT EXISTS submitted_papers (
        id TEXT PRIMARY KEY,
        cid TEXT NOT NULL,
        tid TEXT NOT NULL,
        task_title TEXT,
        questions_json TEXT NOT NULL,
        answers_json TEXT NOT NULL,
        submitted_at_ms INTEGER NOT NULL
    );
-- #[toasty::breakpoint]
    CREATE INDEX IF NOT EXISTS idx_submitted_papers_cid ON submitted_papers(cid);
-- #[toasty::breakpoint]
    CREATE INDEX IF NOT EXISTS idx_submitted_papers_tid ON submitted_papers(tid);
-- #[toasty::breakpoint]
    CREATE TABLE IF NOT EXISTS bank_questions (
        question_id TEXT PRIMARY KEY,
        cid TEXT,
        tid TEXT,
        course_name TEXT,
        task_name TEXT,
        question_type TEXT NOT NULL,
        stem TEXT NOT NULL,
        stem_hash TEXT NOT NULL,
        options_json TEXT,
        answers_json TEXT NOT NULL,
        correct_answers_json TEXT,
        blanks INTEGER NOT NULL,
        uploaded INTEGER NOT NULL,
        embedding_blob BLOB,
        updated_at_ms INTEGER NOT NULL
    );
-- #[toasty::breakpoint]
    CREATE INDEX IF NOT EXISTS idx_bank_questions_cid ON bank_questions(cid);
-- #[toasty::breakpoint]
    CREATE INDEX IF NOT EXISTS idx_bank_questions_tid ON bank_questions(tid);
-- #[toasty::breakpoint]
    CREATE INDEX IF NOT EXISTS idx_bank_questions_stem_hash ON bank_questions(stem_hash);
-- #[toasty::breakpoint]
    CREATE TABLE IF NOT EXISTS video_durations (
        id TEXT PRIMARY KEY,
        duration REAL NOT NULL,
        updated_at_ms INTEGER NOT NULL
    );",
)]);

/// 检查并迁移遗留的 UOOC 视频时长缓存 (video-durations.json / durations.json -> quiz_qa_bank.db)
pub async fn migrate_video_durations_if_needed(
    db_path: &Path,
    db: &QaDatabase,
) -> Result<HashMap<String, f64>, String> {
    let Some(parent) = db_path.parent() else {
        return Ok(HashMap::new());
    };

    let mut migrated = HashMap::new();
    for legacy_name in &["video-durations.json", "durations.json"] {
        let legacy_path = parent.join(legacy_name);
        if legacy_path.exists() {
            if let Ok(bytes) = std::fs::read(&legacy_path) {
                if let Ok(map) = serde_json::from_slice::<HashMap<String, f64>>(&bytes) {
                    for (k, v) in map {
                        if v.is_finite() && v >= 0.0 {
                            if let Some((cid, resource)) = k.split_once(':') {
                                let _ = db.save_video_duration(cid, resource, v).await;
                            }
                            migrated.insert(k, v);
                        }
                    }
                }
            }
            let _ = std::fs::remove_file(legacy_path);
        }
    }

    Ok(migrated)
}

/// 清理遗留的 UOOC QA 题库及向量缓存 JSON 文件
pub fn cleanup_legacy_uooc_files(app_data_dir: &Path) {
    for legacy in &[
        "qa-bank.json",
        "qa-vector-cache.json",
        "video-durations.json",
        "durations.json",
    ] {
        let p = app_data_dir.join(legacy);
        if p.exists() {
            let _ = std::fs::remove_file(p);
        }
    }
}
