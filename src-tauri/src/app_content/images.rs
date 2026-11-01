use super::model::*;
use image::{ImageFormat, ImageReader};
use std::io::Cursor;

/// Longest edge handed to the WebView. The announcement dialog renders the picture at
/// roughly 512 CSS px, so 1600 px still covers high-density screens.
const MAX_IMAGE_EDGE: u32 = 1600;

/// Validate an image response and return the bytes worth caching plus their MIME type.
/// Oversized originals are decoded, downscaled and re-encoded so the WebView never has to
/// receive a multi-megabyte payload. GIFs keep their original bytes to stay animated.
pub(super) fn normalize_image(
    bytes: &[u8],
    content_type: Option<&str>,
) -> Result<(Vec<u8>, &'static str), ContentError> {
    if bytes.len() > IMAGE_DOWNLOAD_LIMIT {
        return Err(ContentError::new(
            ErrorKind::TooLarge,
            "头图超过 16 MiB 限制",
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
    if let Some(value) = content_type {
        let declared = value
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if declared != mime {
            return Err(ContentError::invalid("头图内容与声明的图片类型不匹配"));
        }
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
    let oversized = bytes.len() > IMAGE_LIMIT || width > MAX_IMAGE_EDGE || height > MAX_IMAGE_EDGE;
    if !oversized || format == ImageFormat::Gif {
        if bytes.len() > IMAGE_LIMIT {
            return Err(ContentError::new(
                ErrorKind::TooLarge,
                "头图超过 4 MiB 限制",
            ));
        }
        return Ok((bytes.to_vec(), mime));
    }
    let decoded = ImageReader::with_format(Cursor::new(bytes), format)
        .decode()
        .map_err(|_| ContentError::invalid("无法解码头图"))?;
    let scaled = if width > MAX_IMAGE_EDGE || height > MAX_IMAGE_EDGE {
        decoded.thumbnail(MAX_IMAGE_EDGE, MAX_IMAGE_EDGE)
    } else {
        decoded
    };
    let mut encoded = Vec::new();
    let transparent = has_transparency(&scaled);
    if transparent {
        scaled
            .write_to(&mut Cursor::new(&mut encoded), ImageFormat::Png)
            .map_err(|_| ContentError::invalid("头图重新编码失败"))?;
    } else {
        // Photos re-encoded as PNG stay multi-megabyte; JPEG keeps the data URL small.
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 85)
            .encode_image(&scaled)
            .map_err(|_| ContentError::invalid("头图重新编码失败"))?;
    }
    if encoded.len() > IMAGE_LIMIT {
        return Err(ContentError::new(
            ErrorKind::TooLarge,
            "头图超过 4 MiB 限制",
        ));
    }
    let mime = if transparent {
        "image/png"
    } else {
        "image/jpeg"
    };
    Ok((encoded, mime))
}

/// Alpha channels are common in PNG/WebP even when every pixel is fully opaque, so
/// inspect the pixels instead of trusting the color type.
fn has_transparency(image: &image::DynamicImage) -> bool {
    image.to_rgba8().pixels().any(|pixel| pixel[3] != 255)
}
