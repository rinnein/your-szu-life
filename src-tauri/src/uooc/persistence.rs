use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

pub(super) fn read<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("无法读取 UOOC 本地数据".into()),
    };
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "无法读取 UOOC 本地数据")?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("UOOC 本地数据过大".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "UOOC 本地数据损坏".into())
}

pub(super) fn write(path: &Path, data: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(data).map_err(|_| "UOOC 数据序列化失败")?;
    write_bytes(path, &bytes)
}

pub(super) fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("UOOC 数据目录无效")?;
    let temporary = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(parent)?;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|_| "UOOC 数据保存失败，请检查应用数据目录".into())
}
