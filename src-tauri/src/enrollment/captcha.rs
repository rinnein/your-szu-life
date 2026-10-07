use base64::{engine::general_purpose::STANDARD, Engine};
use des::cipher::{BlockEncrypt, KeyInit};
use image::{DynamicImage, ImageFormat, Luma};
use std::io::Cursor;

// The school DES operates on four UTF-16 code units per block, then base64-encodes
// uppercase hexadecimal ciphertext. This is not UTF-8 DES or standard 3DES.
pub fn password(value: &str) -> String {
    let units: Vec<u16> = value.encode_utf16().collect();
    let mut encoded = String::new();
    for part in units.chunks(4) {
        let mut block = [0u8; 8];
        for (i, u) in part.iter().enumerate() {
            block[i * 2..i * 2 + 2].copy_from_slice(&u.to_be_bytes());
        }
        for key in ["this", "password", "is"] {
            let units: Vec<u16> = key.encode_utf16().collect();
            for chunk in units.chunks(4) {
                let mut bytes = [0u8; 8];
                for (i, u) in chunk.iter().enumerate() {
                    bytes[i * 2..i * 2 + 2].copy_from_slice(&u.to_be_bytes());
                }
                // The deployed JS uses a nonstandard PC-1 ordering. Remap the key
                // into standard DES PC-1 order; rounds/S-boxes remain RustCrypto DES.
                const PC1: [usize; 56] = [
                    56, 48, 40, 32, 24, 16, 8, 0, 57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26,
                    18, 10, 2, 59, 51, 43, 35, 62, 54, 46, 38, 30, 22, 14, 6, 61, 53, 45, 37, 29,
                    21, 13, 5, 60, 52, 44, 36, 28, 20, 12, 4, 27, 19, 11, 3,
                ];
                let mut remapped = [0u8; 8];
                for (i, to) in PC1.iter().enumerate() {
                    let from = 8 * (7 - i % 8) + i / 8;
                    let bit = (bytes[from / 8] >> (7 - from % 8)) & 1;
                    remapped[to / 8] |= bit << (7 - to % 8);
                }
                let cipher = des::Des::new_from_slice(&remapped).unwrap();
                cipher.encrypt_block((&mut block).into());
            }
        }
        for byte in block {
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    STANDARD.encode(encoded)
}
fn intervals(
    image: &image::GrayImage,
    top: u32,
    bottom: u32,
    from: u32,
    to: u32,
) -> Vec<(u32, u32)> {
    let mut ranges = vec![];
    let mut start = None;
    for x in from..to {
        let active = (top..bottom).any(|y| image.get_pixel(x, y)[0] < 140);
        if active && start.is_none() {
            start = Some(x);
        }
        if !active {
            if let Some(left) = start.take() {
                if x - left >= 2 {
                    ranges.push((left, x));
                }
            }
        }
    }
    if let Some(left) = start {
        ranges.push((left, to));
    }
    ranges
}
fn glyph(image: &DynamicImage, x: u32, y: u32, w: u32, h: u32) -> Result<String, String> {
    let crop = image.crop_imm(x, y, w, h).resize_exact(
        w * 5,
        h * 5,
        image::imageops::FilterType::CatmullRom,
    );
    let mut canvas = DynamicImage::ImageLuma8(image::GrayImage::from_pixel(
        w * 5 + 28,
        h * 5 + 28,
        Luma([255]),
    ));
    image::imageops::overlay(&mut canvas, &crop, 14, 14);
    let mut bytes = Cursor::new(Vec::new());
    canvas
        .write_to(&mut bytes, ImageFormat::Png)
        .map_err(|_| "验证码处理失败")?;
    let text = crate::ocr::recognize(bytes.get_ref())?;
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.chars().count() != 1 {
        return Err("验证码字块识别不确定".into());
    }
    Ok(cleaned)
}
#[derive(Debug, PartialEq, Eq)]
pub enum RecognitionError {
    Structure,
    Uncertain,
}
#[derive(Clone, Copy, Debug)]
struct BoxRect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}
fn segmentation(image: &DynamicImage) -> Result<(Vec<BoxRect>, Vec<BoxRect>), RecognitionError> {
    if image.width() != 250 || image.height() != 80 {
        return Err(RecognitionError::Structure);
    }
    let gray = image.to_luma8();
    let binary = imageproc::contrast::threshold(
        &gray,
        imageproc::contrast::otsu_level(&gray),
        imageproc::contrast::ThresholdType::Binary,
    );
    let targets = intervals(&binary, 2, 14, 82, 132)
        .into_iter()
        .map(|(a, b)| BoxRect {
            x: a,
            y: 2,
            w: b - a,
            h: 12,
        })
        .collect::<Vec<_>>();
    if targets.len() != 4 {
        return Err(RecognitionError::Structure);
    }
    let lower = image::imageops::crop_imm(&binary, 0, 25, 250, 55).to_image();
    let labels = imageproc::region_labelling::connected_components(
        &lower,
        imageproc::region_labelling::Connectivity::Eight,
        Luma([255]),
    );
    let mut components = std::collections::BTreeMap::new();
    for (x, y, p) in labels.enumerate_pixels() {
        if p[0] == 0 {
            continue;
        }
        let b = components.entry(p[0]).or_insert((x, y, x, y, 0u32));
        b.0 = b.0.min(x);
        b.1 = b.1.min(y);
        b.2 = b.2.max(x);
        b.3 = b.3.max(y);
        b.4 += 1;
    }
    let mut cleaned = image::GrayImage::from_pixel(250, 80, Luma([255]));
    for (x, y, p) in labels.enumerate_pixels() {
        if components
            .get(&p[0])
            .is_some_and(|b| b.4 >= 3 && b.2 - b.0 < 100)
        {
            cleaned.put_pixel(x, y + 25, Luma([0]));
        }
    }
    let mut boxes = vec![];
    for (left, right) in intervals(&cleaned, 25, 80, 0, 250) {
        if !(8..=60).contains(&(right - left)) {
            continue;
        }
        let rows = (25..80)
            .filter(|y| (left..right).any(|x| cleaned.get_pixel(x, *y)[0] == 0))
            .collect::<Vec<_>>();
        if let (Some(top), Some(bottom)) = (rows.first(), rows.last()) {
            if bottom - top + 1 >= 10 {
                boxes.push(BoxRect {
                    x: left,
                    y: *top,
                    w: right - left,
                    h: bottom - top + 1,
                });
            }
        }
    }
    if boxes.len() < 4 {
        return Err(RecognitionError::Structure);
    }
    Ok((targets, boxes))
}
fn variants(image: &DynamicImage, b: BoxRect) -> Vec<String> {
    let crop = image.crop_imm(b.x, b.y, b.w, b.h);
    let gray = crop.to_luma8();
    let binary = imageproc::contrast::threshold(
        &gray,
        imageproc::contrast::otsu_level(&gray),
        imageproc::contrast::ThresholdType::Binary,
    );
    let mut inverse = binary.clone();
    image::imageops::invert(&mut inverse);
    let mut out = vec![];
    for v in [
        crop,
        DynamicImage::ImageLuma8(binary),
        DynamicImage::ImageLuma8(inverse),
    ] {
        if let Ok(text) = glyph(&v, 0, 0, b.w, b.h) {
            if !out.contains(&text) {
                out.push(text);
            }
        }
    }
    out
}
fn match_unique(targets: &[Vec<String>], candidates: &[Vec<String>]) -> Option<Vec<usize>> {
    fn visit(
        t: &[Vec<String>],
        c: &[Vec<String>],
        path: &mut Vec<usize>,
        solutions: &mut Vec<Vec<usize>>,
    ) {
        if solutions.len() > 1 {
            return;
        }
        if path.len() == t.len() {
            solutions.push(path.clone());
            return;
        }
        for (i, choices) in c.iter().enumerate() {
            if !path.contains(&i) && t[path.len()].iter().any(|v| choices.contains(v)) {
                path.push(i);
                visit(t, c, path, solutions);
                path.pop();
            }
        }
    }
    let mut solutions = vec![];
    visit(targets, candidates, &mut vec![], &mut solutions);
    if solutions.len() == 1 {
        solutions.pop()
    } else {
        None
    }
}
pub fn recognize_next(bytes: &[u8], accepted: &[[u32; 2]]) -> Result<[u32; 2], RecognitionError> {
    if accepted.len() >= 4 || bytes.len() > 1024 * 1024 {
        return Err(RecognitionError::Structure);
    }
    let image = image::load_from_memory(bytes).map_err(|_| RecognitionError::Structure)?;
    let (targets, boxes) = segmentation(&image)?;
    let remaining = boxes
        .into_iter()
        .filter(|b| {
            !accepted
                .iter()
                .any(|p| p[0] >= b.x && p[0] < b.x + b.w && p[1] >= b.y && p[1] < b.y + b.h)
        })
        .collect::<Vec<_>>();
    let target = targets[accepted.len()];
    let wanted = glyph(&image, target.x, target.y, target.w, target.h)
        .ok()
        .into_iter()
        .collect::<Vec<_>>();
    let mut candidates = remaining
        .iter()
        .map(|b| {
            glyph(&image, b.x, b.y, b.w, b.h)
                .ok()
                .into_iter()
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let selected = match_unique(&[wanted.clone()], &candidates)
        .or_else(|| {
            let wanted = variants(&image, target);
            candidates = remaining.iter().map(|b| variants(&image, *b)).collect();
            match_unique(&[wanted], &candidates)
        })
        .ok_or(RecognitionError::Uncertain)?;
    let b = remaining[selected[0]];
    Ok([b.x + b.w / 2, b.y + b.h / 2])
}
pub fn recognize(bytes: &[u8]) -> Result<Vec<[u32; 2]>, RecognitionError> {
    if bytes.len() > 1024 * 1024 {
        return Err(RecognitionError::Structure);
    }
    let image = image::load_from_memory(bytes).map_err(|_| RecognitionError::Structure)?;
    let (targets, boxes) = segmentation(&image)?;
    let first = |b: &BoxRect| {
        glyph(&image, b.x, b.y, b.w, b.h)
            .ok()
            .into_iter()
            .collect::<Vec<_>>()
    };
    let mut t = targets.iter().map(first).collect::<Vec<_>>();
    let mut c = boxes.iter().map(first).collect::<Vec<_>>();
    let matching = match_unique(&t, &c)
        .or_else(|| {
            t = targets.iter().map(|b| variants(&image, *b)).collect();
            c = boxes.iter().map(|b| variants(&image, *b)).collect();
            match_unique(&t, &c)
        })
        .ok_or(RecognitionError::Uncertain)?;
    let points = matching
        .iter()
        .map(|i| {
            let b = boxes[*i];
            [b.x + b.w / 2, b.y + b.h / 2]
        })
        .collect::<Vec<_>>();
    coordinates(&points).map_err(|_| RecognitionError::Uncertain)?;
    Ok(points)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "loads the OCR model; synthetic accuracy benchmark"]
    fn synthetic_benchmark() {
        let samples: [&[u8]; 6] = [
            include_bytes!("fixtures/captcha/0.png"),
            include_bytes!("fixtures/captcha/1.png"),
            include_bytes!("fixtures/captcha/2.png"),
            include_bytes!("fixtures/captcha/3.png"),
            include_bytes!("fixtures/captcha/4.png"),
            include_bytes!("fixtures/captcha/5.png"),
        ];
        fn baseline(bytes: &[u8]) -> Result<Vec<[u32; 2]>, String> {
            let image = image::load_from_memory(bytes).unwrap();
            let gray = image.to_luma8();
            let top = intervals(&gray, 2, 14, 1, 249);
            if top.len() != 4 {
                return Err("structure".into());
            }
            let targets = top
                .iter()
                .map(|(a, b)| glyph(&image, *a, 2, b - a, 12))
                .collect::<Result<Vec<_>, _>>()?;
            let mut candidates = vec![];
            for (a, b) in intervals(&gray, 20, 79, 1, 249) {
                if !(6..=60).contains(&(b - a)) {
                    continue;
                }
                let rows = (20..79)
                    .filter(|y| (a..b).any(|x| gray.get_pixel(x, *y)[0] < 140))
                    .collect::<Vec<_>>();
                if let (Some(y1), Some(y2)) = (rows.first(), rows.last()) {
                    if let Ok(text) = glyph(&image, a, *y1, b - a, y2 - y1 + 1) {
                        candidates.push((text, [(a + b) / 2, (y1 + y2) / 2]));
                    }
                }
            }
            let mut out = vec![];
            for t in targets {
                let matches = candidates
                    .iter()
                    .filter(|(s, p)| s == &t && !out.contains(p))
                    .collect::<Vec<_>>();
                if matches.len() != 1 {
                    return Err("uncertain".into());
                }
                out.push(matches[0].1);
            }
            Ok(out)
        }
        for new in [false, true] {
            let start = std::time::Instant::now();
            let mut correct = 0;
            let mut rejected = 0;
            for sample in samples {
                let result = if new {
                    recognize(sample).map_err(|_| "rejected".to_string())
                } else {
                    baseline(sample)
                };
                match result {
                    Ok(points) => {
                        if points.iter().zip([1, 3, 0, 2]).all(|(p, j)| {
                            p[0] >= 10 + 58 * j && p[0] <= 38 + 58 * j && p[1] >= 30 && p[1] <= 58
                        }) {
                            correct += 1;
                        }
                    }
                    Err(_) => rejected += 1,
                }
            }
            println!(
                "synthetic new={new}: correct={correct}/6 rejected={rejected}/6 elapsed_ms={}",
                start.elapsed().as_millis()
            );
        }
    }
    #[test]
    fn unique_matching_rejects_ambiguous_candidates() {
        let t = vec![vec!["甲".into()], vec!["乙".into()]];
        assert_eq!(
            match_unique(&t, &[vec!["乙".into()], vec!["甲".into()]]),
            Some(vec![1, 0])
        );
        assert_eq!(
            match_unique(
                &t,
                &[vec!["甲".into()], vec!["甲".into()], vec!["乙".into()]]
            ),
            None
        );
    }
    #[test]
    fn target_crop_ignores_border_and_prompt() {
        let mut img = image::GrayImage::from_pixel(250, 80, Luma([255]));
        for x in 0..250 {
            img.put_pixel(x, 0, Luma([0]));
        }
        for (x, y, w, h) in [
            (5, 2, 60, 12),
            (84, 3, 8, 10),
            (96, 3, 8, 10),
            (108, 3, 8, 10),
            (120, 3, 8, 10),
            (10, 30, 16, 25),
            (65, 35, 16, 25),
            (120, 30, 16, 25),
            (190, 35, 16, 25),
        ] {
            for xx in x..x + w {
                for yy in y..y + h {
                    img.put_pixel(xx, yy, Luma([0]));
                }
            }
        }
        let (targets, boxes) = segmentation(&DynamicImage::ImageLuma8(img)).unwrap();
        assert_eq!(targets.len(), 4);
        assert_eq!(boxes.len(), 4);
        assert_eq!(targets[0].x, 84);
    }
}
pub fn coordinates(points: &[[u32; 2]]) -> Result<String, String> {
    if points.len() != 4
        || points.iter().any(|p| p[0] >= 250 || p[1] >= 80)
        || points
            .iter()
            .enumerate()
            .any(|(i, p)| points[..i].contains(p))
    {
        return Err("请依次点选四个不同的字".into());
    }
    Ok(points
        .iter()
        .map(|p| format!("{}-{}", p[0], p[1]))
        .collect::<Vec<_>>()
        .join(","))
}
