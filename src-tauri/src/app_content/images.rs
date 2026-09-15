use super::model::*;
use base64::Engine;
use image::{ImageFormat, ImageReader};
use std::io::Cursor;

pub(super) fn image_data_url(
    bytes: &[u8],
    content_type: Option<&str>,
) -> Result<String, ContentError> {
    if bytes.len() > IMAGE_LIMIT {
        return Err(ContentError::new(
            ErrorKind::TooLarge,
            "头图超过 4 MiB 限制",
        ));
    }
    let format =
        image::guess_format(bytes).map_err(|_| ContentError::invalid("头图不是支持的图片格式"))?;
    let mime = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Gif => "image/gif",
        ImageFormat::WebP => "image/webp",
        _ => return Err(ContentError::invalid("仅支持 PNG、JPEG、GIF、WebP 头图")),
    };
    if content_type.is_some_and(|value| {
        value
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            != mime
    }) {
        return Err(ContentError::invalid("头图内容与声明的图片类型不匹配"));
    }
    let (width, height) = ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|_| ContentError::invalid("无法读取头图尺寸"))?;
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > 32_000_000
    {
        return Err(ContentError::invalid("头图尺寸超限"));
    }
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}
