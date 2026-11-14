//! 签到照片的压缩与格式归一（SZU 拍照/上传签到用）。
//!
//! 与公告头图（szu-app-content::images::normalize_image）同一思路：
//! 超限的原图解码 → thumbnail（最长边 1600px）→ JPEG 质量 85 重编码，
//! 保证 multipart 上传的体积可控、格式统一为 JPEG。

use image::{ImageFormat, ImageReader};
use std::io::Cursor;

const MAX_IMAGE_EDGE: u32 = 1600;
const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

/// 归一化签到照片：任何可解码输入 → JPEG（含透明度时转白底）。
/// 返回 (JPEG 字节, mime)。
pub fn normalize_photo(bytes: &[u8]) -> Result<(Vec<u8>, &'static str), String> {
    if bytes.is_empty() {
        return Err("照片内容为空".into());
    }
    if bytes.len() > MAX_INPUT_BYTES {
        return Err("照片超过 16 MiB 限制".into());
    }
    let format =
        image::guess_format(bytes).map_err(|_| "照片不是支持的图片格式".to_string())?;
    let decoded = ImageReader::with_format(Cursor::new(bytes), format)
        .decode()
        .map_err(|_| "无法解码照片".to_string())?;
    let scaled = if decoded.width() > MAX_IMAGE_EDGE || decoded.height() > MAX_IMAGE_EDGE {
        decoded.thumbnail(MAX_IMAGE_EDGE, MAX_IMAGE_EDGE)
    } else {
        decoded
    };
    // 签到照片统一转白底 RGB，避免 RGBA JPEG 编码歧义。
    let rgb = image::DynamicImage::ImageRgb8(scaled.to_rgb8());
    let mut encoded = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 85)
        .encode_image(&rgb)
        .map_err(|_| "照片重编码失败".to_string())?;
    Ok((encoded, "image/jpeg"))
}

/// 缩略图 data URI（前端预览「上次照片」用）。
pub fn photo_thumbnail_data_uri(jpeg: &[u8]) -> Result<String, String> {
    let decoded = ImageReader::with_format(Cursor::new(jpeg), ImageFormat::Jpeg)
        .decode()
        .map_err(|_| "无法解码已有照片".to_string())?;
    let thumb = decoded.thumbnail(320, 320);
    let mut encoded = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 80)
        .encode_image(&thumb)
        .map_err(|_| "缩略图编码失败".to_string())?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64_encode(&encoded)
    ))
}

pub fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn base64_decode(raw: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .map_err(|_| "照片数据不是有效的 base64".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg_bytes(width: u32, height: u32) -> Vec<u8> {
        let image = image::DynamicImage::new_rgb8(width, height);
        let mut out = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Jpeg)
            .unwrap();
        out
    }

    #[test]
    fn oversized_photo_is_downscaled_to_jpeg() {
        let original = jpeg_bytes(2400, 3200);
        let (encoded, mime) = normalize_photo(&original).expect("归一化应成功");
        assert_eq!(mime, "image/jpeg");
        let (width, height) = ImageReader::with_format(Cursor::new(&encoded), ImageFormat::Jpeg)
            .into_dimensions()
            .unwrap();
        assert!(width.max(height) <= MAX_IMAGE_EDGE, "{width}x{height}");
    }

    #[test]
    fn small_photo_is_re_encoded_as_jpeg() {
        let original = jpeg_bytes(800, 600);
        let (encoded, mime) = normalize_photo(&original).unwrap();
        assert_eq!(mime, "image/jpeg");
        assert!(!encoded.is_empty());
    }

    #[test]
    fn rejects_empty_and_unknown_bytes() {
        assert!(normalize_photo(&[]).is_err());
        assert!(normalize_photo(b"not an image").is_err());
    }

    #[test]
    fn base64_roundtrip() {
        let raw = b"photo-bytes";
        let encoded = base64_encode(raw);
        assert_eq!(base64_decode(&encoded).unwrap(), raw);
        assert!(base64_decode("!!!").is_err());
    }
}
