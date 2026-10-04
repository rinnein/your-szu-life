use super::model::Data;
use std::{io::Write, path::Path};
pub(super) fn read(path: &Path) -> Result<Data, String> {
    match std::fs::read(path) {
        Ok(bytes) => {
            if bytes.len() > 32 * 1024 * 1024 {
                return Err("OJ 本地数据过大".into());
            }
            let data: Data =
                serde_json::from_slice(&bytes).map_err(|_| "OJ 本地数据损坏，原文件已保留")?;
            if data.version != 1 {
                return Err("OJ 本地数据版本不兼容".into());
            }
            Ok(data)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Data::default()),
        Err(_) => Err("无法读取 OJ 本地数据".into()),
    }
}
pub(super) fn write(path: &Path, data: &Data) -> Result<(), String> {
    let parent = path.parent().ok_or("数据目录无效")?;
    std::fs::create_dir_all(parent).map_err(|_| "无法创建 OJ 数据目录")?;
    let temp = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        serde_json::to_writer(&mut file, data)?;
        file.flush()?;
        file.sync_all()?;
        std::fs::rename(&temp, path)?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result.map_err(|_| "无法保存 OJ 数据，请检查存储空间".into())
}
