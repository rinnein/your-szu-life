//! 目录与路径规则。
//!
//! 这里只放纯函数，方便跨平台单测；平台差异（选择器、SAF 调用）在
//! `desktop` / `android` 子模块里。

use std::path::{Path, PathBuf};

/// 统一目录的品牌子目录。
pub(crate) const BRAND_DIR: &str = "泥深";

/// 目录里是否存在任意子项（含隐藏文件）。
///
/// Android 的目录枚举由 Kotlin 侧的 SAF 查询完成，这里只服务桌面端。
#[cfg(not(target_os = "android"))]
pub(crate) fn has_entries(dir: &Path) -> Result<bool, String> {
    let mut entries =
        std::fs::read_dir(dir).map_err(|error| format!("无法读取目录 {}: {error}", dir.display()))?;
    Ok(entries.next().is_some())
}

/// 按「空目录直接使用、非空目录追加品牌子目录」的规则解析父目录。
pub(crate) fn resolve_local_base(selected: &Path, has_entries: bool) -> PathBuf {
    if has_entries {
        selected.join(BRAND_DIR)
    } else {
        selected.to_path_buf()
    }
}

/// 拼接本地目录：忽略空片段。
pub(crate) fn join_local(base: &Path, segments: &[&str]) -> PathBuf {
    segments
        .iter()
        .filter(|segment| !segment.is_empty())
        .fold(base.to_path_buf(), |acc, segment| acc.join(segment))
}

/// 拼接展示路径（Android 与桌面分别使用各自的路径分隔符）。
pub(crate) fn join_display(root: &str, segment: &str) -> String {
    let trimmed = root.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        segment.to_string()
    } else {
        format!("{trimmed}{}{segment}", std::path::MAIN_SEPARATOR)
    }
}

/// 拼接多个展示路径片段。
#[cfg(any(target_os = "android", test))]
pub(crate) fn join_display_segments(root: &str, segments: &[&str]) -> String {
    segments
        .iter()
        .filter(|segment| !segment.is_empty())
        .fold(root.to_string(), |acc, segment| join_display(&acc, segment))
}

/// 拼接 SAF 树内相对路径（始终用 `/`，与文档 URI 无关）。
#[cfg(any(target_os = "android", test))]
pub(crate) fn join_relative(base: &str, segments: &[&str]) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if !base.is_empty() {
        parts.push(base);
    }
    parts.extend(segments.iter().copied().filter(|segment| !segment.is_empty()));
    parts.join("/")
}

/// 文件名/目录名清洗：去掉路径分隔符与文件系统保留字符。
pub(crate) fn sanitize_segment(raw: &str) -> String {
    raw.chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_string()
}

/// 创建目录并写一个探针文件，确认真的可写。
pub(crate) fn probe_writable(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("无法创建目录 {}: {error}", dir.display()))?;
    let probe = dir.join(".泥深-write-test");
    std::fs::write(&probe, b"ok")
        .map_err(|error| format!("目录不可写 {}: {error}", dir.display()))?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_selection_uses_the_directory_itself() {
        let selected = PathBuf::from("/tmp/empty");
        assert_eq!(resolve_local_base(&selected, false), selected);
    }

    #[test]
    fn non_empty_selection_appends_the_brand_directory() {
        let selected = PathBuf::from("/tmp/used");
        assert_eq!(
            resolve_local_base(&selected, true),
            selected.join(BRAND_DIR)
        );
    }

    #[test]
    fn joins_relative_paths_without_double_separators() {
        assert_eq!(join_relative("", &["jxxk"]), "jxxk");
        assert_eq!(join_relative("泥深", &["jxxk"]), "泥深/jxxk");
        assert_eq!(join_relative("泥深", &["", "第1节"]), "泥深/第1节");
        assert_eq!(
            join_relative("泥深/jxxk", &["课程", "第1节"]),
            "泥深/jxxk/课程/第1节"
        );
    }

    #[test]
    fn display_join_trims_trailing_separators() {
        let module = String::from("jx") + "xk";
        let joined = join_display("/sdcard/Download/泥深/", &module);
        assert_eq!(
            joined,
            format!("/sdcard/Download/泥深{}{module}", std::path::MAIN_SEPARATOR)
        );
        assert_eq!(join_display("", &module), module);
    }

    #[test]
    fn sanitize_replaces_reserved_characters() {
        assert_eq!(sanitize_segment("a/b:c*?\"<>|d"), "a_b_c______d");
        assert_eq!(sanitize_segment("  ..第1节..  "), "第1节");
        assert_eq!(sanitize_segment("..."), "");
    }

    #[test]
    fn probe_reports_missing_permissions_instead_of_silently_succeeding() {
        let dir = std::env::temp_dir().join("your-szu-life-file-manager-test");
        probe_writable(&dir).unwrap();
        assert!(dir.is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
