//! 桌面端当前壁纸的读取与缩略，供前端用 Material Color Utilities 提取种子色。

use super::WallpaperImage;
use base64::Engine;
use std::{io::Cursor, path::Path};

const MAX_WALLPAPER_BYTES: u64 = 64 * 1024 * 1024;
const THUMBNAIL_EDGE: u32 = 256;

pub(super) fn current_wallpaper_image() -> Result<WallpaperImage, String> {
    let path = wallpaper::get().map_err(|error| format!("无法读取当前桌面壁纸：{error}"))?;
    encode_wallpaper(Path::new(&path))
}

fn encode_wallpaper(path: &Path) -> Result<WallpaperImage, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("无法读取当前壁纸文件：{error}"))?;
    if metadata.len() > MAX_WALLPAPER_BYTES {
        return Err("当前壁纸文件过大，无法自动读取，请手动上传图片".into());
    }
    let image = image::open(path)
        .map_err(|_| "当前壁纸不是可识别的图片格式，请手动上传图片".to_string())?;
    let thumbnail = image.thumbnail(THUMBNAIL_EDGE, THUMBNAIL_EDGE);
    let mut bytes = Vec::new();
    thumbnail
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .map_err(|error| format!("壁纸缩略图生成失败：{error}"))?;
    let data_url = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    );
    Ok(WallpaperImage {
        data_url,
        width: thumbnail.width(),
        height: thumbnail.height(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_bounded_png_data_url() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wallpaper.png");
        let image = image::RgbaImage::from_fn(1000, 500, |x, y| {
            image::Rgba([(x % 255) as u8, (y % 255) as u8, 40, 255])
        });
        image::DynamicImage::ImageRgba8(image).save(&path).unwrap();

        let encoded = encode_wallpaper(&path).unwrap();
        assert!(encoded.data_url.starts_with("data:image/png;base64,"));
        assert!(encoded.width <= THUMBNAIL_EDGE);
        assert!(encoded.height <= THUMBNAIL_EDGE);
        assert_eq!(encoded.width, encoded.height * 2);
    }

    #[test]
    fn rejects_non_image_wallpaper() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wallpaper.txt");
        std::fs::write(&path, b"not an image").unwrap();
        assert!(encode_wallpaper(&path).is_err());
    }
}
