//! 日志与本地模型的旧位置 → 新位置一次性迁移。
//!
//! Android 上从应用内部存储搬到应用专属外部目录；桌面把模型从数据根目录
//! 收拢到 `local_model/`。规则：先复制到目标目录的临时文件，校验字节数一致
//! 后原子改名，确认目标文件落盘后才删除源文件；任何失败都保留源文件，下次
//! 启动重试。

use std::{fs, io, path::Path};
use tauri::{AppHandle, Manager};

/// 旧版摊在数据根目录下的模型子目录：`<model>/<variant>`。
const LEGACY_MODEL_DIRS: [(&str, &str); 2] =
    [("ocr", "ddddocr"), ("embedding", "bge-small-zh-v1.5")];

#[derive(Default)]
pub(crate) struct StorageMigrationReport {
    pub(crate) copied: usize,
    pub(crate) removed: usize,
    pub(crate) failures: Vec<String>,
}

#[derive(Clone, Copy)]
enum ExistingTarget {
    /// 目标目录是权威副本（日志）：目标已存在就只删除源文件。
    PreferTarget,
    /// 目标必须是同一份文件（模型）：大小一致才删除源文件。
    SameSize,
}

pub(crate) fn migrate_legacy_storage(app: &AppHandle) -> StorageMigrationReport {
    let mut report = StorageMigrationReport::default();

    match (app.path().app_log_dir(), crate::app_paths::log_dir(app)) {
        (Ok(source), Ok(target)) => {
            migrate_dir(&source, &target, ExistingTarget::PreferTarget, &mut report)
        }
        _ => report.failures.push("无法确定日志目录".to_string()),
    }

    let legacy_root = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(error) => {
            report
                .failures
                .push(format!("无法确定应用数据目录：{error}"));
            return report;
        }
    };
    let new_root = match crate::app_paths::local_model_dir(app) {
        Ok(dir) => dir,
        Err(error) => {
            report
                .failures
                .push(format!("无法确定本地模型目录：{error}"));
            return report;
        }
    };

    for (model, variant) in LEGACY_MODEL_DIRS {
        migrate_dir(
            &legacy_root.join(model).join(variant),
            &new_root.join(model).join(variant),
            ExistingTarget::SameSize,
            &mut report,
        );
        // 只在子目录已经搬空时删除；有未知文件就保留。
        let _ = fs::remove_dir(legacy_root.join(model));
    }

    for failure in &report.failures {
        log::warn!("存储迁移未完成：{failure}");
    }
    if report.copied > 0 || report.removed > 0 {
        log::info!(
            "存储迁移完成：复制 {} 个文件，删除 {} 个旧文件",
            report.copied,
            report.removed
        );
    }

    report
}

/// 把 `source` 下的普通文件逐个复制到 `target`，随后删除源文件。
fn migrate_dir(
    source: &Path,
    target: &Path,
    policy: ExistingTarget,
    report: &mut StorageMigrationReport,
) {
    if source == target || !source.is_dir() {
        return;
    }
    let entries = match fs::read_dir(source) {
        Ok(entries) => entries,
        Err(error) => {
            report
                .failures
                .push(format!("无法读取旧目录 {}：{error}", source.display()));
            return;
        }
    };

    for entry in entries.flatten() {
        let source_path = entry.path();
        if !source_path.is_file() {
            continue;
        }
        let Some(name) = source_path.file_name() else {
            continue;
        };
        let target_path = target.join(name);
        let source_size = match fs::metadata(&source_path) {
            Ok(metadata) => metadata.len(),
            Err(error) => {
                report
                    .failures
                    .push(format!("无法读取旧文件 {}：{error}", source_path.display()));
                continue;
            }
        };

        if target_path.exists() {
            match policy {
                ExistingTarget::PreferTarget => remove_source(&source_path, report),
                ExistingTarget::SameSize => {
                    let target_size = fs::metadata(&target_path).ok().map(|m| m.len());
                    if target_size == Some(source_size) {
                        remove_source(&source_path, report);
                    } else {
                        report.failures.push(format!(
                            "目标文件已存在但大小不同，保留源文件：{}",
                            source_path.display()
                        ));
                    }
                }
            }
            continue;
        }

        if let Err(error) = fs::create_dir_all(target) {
            report
                .failures
                .push(format!("无法创建目标目录 {}：{error}", target.display()));
            continue;
        }

        let temporary = target.join(format!(".{}.migrating", name.to_string_lossy()));
        if let Err(error) = fs::copy(&source_path, &temporary) {
            let _ = fs::remove_file(&temporary);
            report
                .failures
                .push(format!("复制 {} 失败：{error}", source_path.display()));
            continue;
        }
        let copied_size = fs::metadata(&temporary).ok().map(|m| m.len());
        if copied_size != Some(source_size) {
            let _ = fs::remove_file(&temporary);
            report
                .failures
                .push(format!("复制 {} 后大小不一致", source_path.display()));
            continue;
        }
        if let Err(error) = fs::rename(&temporary, &target_path) {
            let _ = fs::remove_file(&temporary);
            report
                .failures
                .push(format!("安装 {} 失败：{error}", target_path.display()));
            continue;
        }

        report.copied += 1;
        remove_source(&source_path, report);
    }

    // 只删除已经搬空的目录；未知文件或子目录会让它保留下来。
    let _ = fs::remove_dir(source);
}

fn remove_source(path: &Path, report: &mut StorageMigrationReport) {
    match fs::remove_file(path) {
        Ok(()) => report.removed += 1,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => report.failures.push(format!(
            "已复制但无法删除源文件 {}：{error}",
            path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(path: &Path, contents: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn copies_then_deletes_source_file_and_directory() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("old/logs");
        let target = dir.path().join("new/files/logs");
        write(&source.join("a.log"), b"hello");

        let mut report = StorageMigrationReport::default();
        migrate_dir(&source, &target, ExistingTarget::PreferTarget, &mut report);

        assert_eq!(fs::read(target.join("a.log")).unwrap(), b"hello");
        assert!(!source.join("a.log").exists());
        assert!(!source.exists());
        assert_eq!(report.copied, 1);
        assert_eq!(report.removed, 1);
        assert!(report.failures.is_empty());
    }

    #[test]
    fn existing_target_with_same_size_keeps_target_and_removes_source() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("old");
        let target = dir.path().join("new");
        write(&source.join("a.log"), b"new");
        write(&target.join("a.log"), b"old");

        let mut report = StorageMigrationReport::default();
        migrate_dir(&source, &target, ExistingTarget::SameSize, &mut report);

        assert_eq!(fs::read(target.join("a.log")).unwrap(), b"old");
        assert!(!source.join("a.log").exists());
        assert_eq!(report.copied, 0);
        assert_eq!(report.removed, 1);
    }

    #[test]
    fn existing_target_with_different_size_keeps_source() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("old");
        let target = dir.path().join("new");
        write(&source.join("a.log"), b"newer");
        write(&target.join("a.log"), b"old");

        let mut report = StorageMigrationReport::default();
        migrate_dir(&source, &target, ExistingTarget::SameSize, &mut report);

        assert!(source.join("a.log").exists());
        assert_eq!(fs::read(target.join("a.log")).unwrap(), b"old");
        assert_eq!(report.failures.len(), 1);
    }

    #[test]
    fn log_target_wins_even_when_sizes_differ() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("old");
        let target = dir.path().join("new");
        write(&source.join("a.log"), b"old");
        write(&target.join("a.log"), b"old plus new writes");

        let mut report = StorageMigrationReport::default();
        migrate_dir(&source, &target, ExistingTarget::PreferTarget, &mut report);

        assert!(!source.join("a.log").exists());
        assert_eq!(
            fs::read(target.join("a.log")).unwrap(),
            b"old plus new writes"
        );
        assert_eq!(report.copied, 0);
        assert_eq!(report.removed, 1);
        assert!(report.failures.is_empty());
    }

    #[test]
    fn keeps_source_directory_when_unknown_entries_remain() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("old");
        let target = dir.path().join("new");
        write(&source.join("a.log"), b"hello");
        fs::create_dir_all(source.join("nested")).unwrap();

        let mut report = StorageMigrationReport::default();
        migrate_dir(&source, &target, ExistingTarget::PreferTarget, &mut report);

        assert!(target.join("a.log").exists());
        assert!(source.exists());
        assert!(source.join("nested").exists());
    }

    #[test]
    fn copy_failure_keeps_source_file() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("old");
        let target = dir.path().join("new");
        write(&source.join("a.log"), b"hello");
        write(&target, b"not a directory");

        let mut report = StorageMigrationReport::default();
        migrate_dir(&source, &target, ExistingTarget::PreferTarget, &mut report);

        assert!(source.join("a.log").exists());
        assert_eq!(fs::read(&target).unwrap(), b"not a directory");
        assert_eq!(report.copied, 0);
        assert_eq!(report.failures.len(), 1);
    }

    #[test]
    fn equal_source_and_target_is_a_noop() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("same");
        write(&source.join("a.log"), b"hello");

        let mut report = StorageMigrationReport::default();
        migrate_dir(&source, &source, ExistingTarget::PreferTarget, &mut report);

        assert!(source.join("a.log").exists());
        assert!(report.failures.is_empty());
    }
}
