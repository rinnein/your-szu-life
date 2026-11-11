use std::collections::HashMap;

pub(super) struct Document<'a> {
    pub(super) metas: Vec<&'a str>,
    pub(super) scripts: Vec<&'a str>,
}

fn tag_end(html: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, byte) in html.as_bytes()[start..].iter().copied().enumerate() {
        if quote == Some(byte) {
            quote = None;
        } else if quote.is_none() {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'>' => return Some(start + offset),
                _ => {}
            }
        }
    }
    None
}

pub(super) fn parse(html: &str) -> Document<'_> {
    let lower = html.to_ascii_lowercase();
    let mut document = Document {
        metas: Vec::new(),
        scripts: Vec::new(),
    };
    let mut position = 0;
    while let Some(offset) = lower[position..].find('<') {
        let start = position + offset;
        if lower[start..].starts_with("<!--") {
            let Some(end) = lower[start + 4..].find("-->") else {
                break;
            };
            position = start + 4 + end + 3;
            continue;
        }
        let name_start = start + 1;
        let name_end = lower.as_bytes()[name_start..]
            .iter()
            .position(|byte| !byte.is_ascii_alphanumeric() && *byte != b'-')
            .map(|offset| name_start + offset)
            .unwrap_or(lower.len());
        let name = &lower[name_start..name_end];
        let Some(end) = tag_end(html, name_end) else {
            break;
        };
        let attrs = &html[name_end..end];
        position = end + 1;
        if name == "meta" {
            document.metas.push(attrs);
        } else if matches!(name, "script" | "style" | "textarea" | "title") {
            let closing = format!("</{name}");
            let mut search = position;
            let close_start = loop {
                let Some(offset) = lower[search..].find(&closing) else {
                    return document;
                };
                let candidate = search + offset;
                let next = lower.as_bytes().get(candidate + closing.len());
                if next.is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'>') {
                    break candidate;
                }
                search = candidate + closing.len();
            };
            if name == "script" {
                let attributes = attributes(attrs);
                let executable = !attributes.contains_key("src")
                    && attributes.get("type").is_none_or(|kind| {
                        matches!(
                            kind.trim().to_ascii_lowercase().as_str(),
                            "" | "text/javascript"
                                | "application/javascript"
                                | "text/ecmascript"
                                | "application/ecmascript"
                                | "module"
                        )
                    });
                if executable {
                    document.scripts.push(&html[position..close_start]);
                }
            }
            let Some(close_end) = tag_end(html, close_start + closing.len()) else {
                break;
            };
            position = close_end + 1;
        }
    }
    document
}

pub(super) fn attributes(raw: &str) -> HashMap<String, String> {
    let bytes = raw.as_bytes();
    let mut result = HashMap::new();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && (bytes[index].is_ascii_whitespace() || bytes[index] == b'/') {
            index += 1;
        }
        let start = index;
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'=' {
            index += 1;
        }
        if start == index {
            index += 1;
            continue;
        }
        let key = raw[start..index].to_ascii_lowercase();
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let mut value = String::new();
        if bytes.get(index) == Some(&b'=') {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            let quote = bytes
                .get(index)
                .copied()
                .filter(|byte| matches!(byte, b'\'' | b'"'));
            if quote.is_some() {
                index += 1;
            }
            let start = index;
            while index < bytes.len()
                && match quote {
                    Some(quote) => bytes[index] != quote,
                    None => !bytes[index].is_ascii_whitespace(),
                }
            {
                index += 1;
            }
            value = raw[start..index].to_owned();
            if quote.is_some() && index < bytes.len() {
                index += 1;
            }
        }
        result.entry(key).or_insert(value);
    }
    result
}

pub(super) fn content_type_charset(content_type: &str) -> Option<String> {
    content_type
        .split(';')
        .filter_map(|part| part.split_once('='))
        .find_map(|(key, value)| {
            let value = value.trim().trim_matches(['\'', '"']).trim();
            (key.trim().eq_ignore_ascii_case("charset") && !value.is_empty())
                .then(|| value.to_ascii_lowercase())
        })
}

pub(super) fn meta_charset(html: &str) -> Option<String> {
    parse(html).metas.into_iter().find_map(|meta| {
        let attrs = attributes(meta);
        attrs
            .get("charset")
            .map(|label| label.trim().to_ascii_lowercase())
            .filter(|label| !label.is_empty())
            .or_else(|| {
                attrs
                    .get("http-equiv")
                    .filter(|value| value.eq_ignore_ascii_case("content-type"))?;
                content_type_charset(attrs.get("content")?)
            })
    })
}

fn decode_attribute_entities(value: &str) -> String {
    let mut result = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        result.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let decoded = tail.find(';').filter(|end| *end <= 12).and_then(|end| {
            let entity = &tail[..end];
            let character = match entity {
                "amp" => Some('&'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "lt" => Some('<'),
                "gt" => Some('>'),
                _ => entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                    .and_then(|digits| u32::from_str_radix(digits, 16).ok())
                    .or_else(|| {
                        entity
                            .strip_prefix('#')
                            .and_then(|digits| digits.parse().ok())
                    })
                    .and_then(char::from_u32),
            }?;
            Some((end, character))
        });
        if let Some((end, character)) = decoded {
            result.push(character);
            rest = &tail[end + 1..];
        } else {
            result.push('&');
            rest = tail;
        }
    }
    result.push_str(rest);
    result
}

pub(super) fn meta_refresh_target(html: &str) -> Option<String> {
    parse(html).metas.into_iter().find_map(|meta| {
        let attrs = attributes(meta);
        if !attrs
            .get("http-equiv")?
            .trim()
            .eq_ignore_ascii_case("refresh")
        {
            return None;
        }
        let (_, target) = attrs.get("content")?.split_once(';')?;
        let (key, value) = target.trim().split_once('=')?;
        if !key.trim().eq_ignore_ascii_case("url") {
            return None;
        }
        let value = decode_attribute_entities(value.trim().trim_matches(['\'', '"']));
        (!value.is_empty()).then_some(value)
    })
}
