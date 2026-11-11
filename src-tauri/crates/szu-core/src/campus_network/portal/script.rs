use super::document;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub(super) enum Value {
    Text(String),
    Number(u64),
    Bool(bool),
}

impl Value {
    pub(super) fn text(&self) -> Option<String> {
        match self {
            Self::Text(text) => {
                let text = text.trim();
                (!text.is_empty()).then(|| text.to_owned())
            }
            _ => None,
        }
    }
    pub(super) fn number(&self) -> Option<u64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Text(text) => text.trim().parse().ok(),
            Self::Bool(_) => None,
        }
    }
}

#[derive(Debug)]
enum Token {
    Identifier(String),
    Literal(Value),
    Punctuation(u8),
    Other,
}

fn identifier(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
}

fn hex(script: &str, index: &mut usize, length: usize) -> Option<u16> {
    let digits = script.get(*index..*index + length)?;
    if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    *index += length;
    u16::from_str_radix(digits, 16).ok()
}

fn string(script: &str, index: &mut usize) -> Option<String> {
    let bytes = script.as_bytes();
    let quote = bytes[*index];
    *index += 1;
    let mut units = Vec::new();
    while *index < bytes.len() {
        let byte = bytes[*index];
        *index += 1;
        if byte == quote {
            return String::from_utf16(&units).ok();
        }
        if matches!(byte, b'\n' | b'\r') {
            return None;
        }
        if byte == b'\\' {
            let escape = *bytes.get(*index)?;
            *index += 1;
            let unit = match escape {
                b'\n' => continue,
                b'\r' => {
                    if bytes.get(*index) == Some(&b'\n') {
                        *index += 1;
                    }
                    continue;
                }
                b'n' => b'\n' as u16,
                b'r' => b'\r' as u16,
                b't' => b'\t' as u16,
                b'b' => 8,
                b'f' => 12,
                b'v' => 11,
                b'0' => 0,
                b'x' => hex(script, index, 2)?,
                b'u' if bytes.get(*index) == Some(&b'{') => {
                    *index += 1;
                    let end = script.get(*index..)?.find('}')? + *index;
                    let scalar = u32::from_str_radix(script.get(*index..end)?, 16).ok()?;
                    let character = char::from_u32(scalar)?;
                    let mut buffer = [0; 2];
                    units.extend_from_slice(character.encode_utf16(&mut buffer));
                    *index = end + 1;
                    continue;
                }
                b'u' => hex(script, index, 4)?,
                _ if escape.is_ascii() => escape as u16,
                _ => {
                    *index -= 1;
                    let character = script.get(*index..)?.chars().next()?;
                    let mut buffer = [0; 2];
                    units.extend_from_slice(character.encode_utf16(&mut buffer));
                    *index += character.len_utf8();
                    continue;
                }
            };
            units.push(unit);
        } else {
            *index -= 1;
            let character = script.get(*index..)?.chars().next()?;
            let mut buffer = [0; 2];
            units.extend_from_slice(character.encode_utf16(&mut buffer));
            *index += character.len_utf8();
        }
    }
    None
}

fn tokens(script: &str) -> Vec<Token> {
    let bytes = script.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if script[index..].starts_with("//")
            || script[index..].starts_with("<!--")
            || script[index..].starts_with("-->")
        {
            index += script[index..]
                .find(['\r', '\n'])
                .unwrap_or(script.len() - index);
        } else if script[index..].starts_with("/*") {
            let Some(end) = script[index + 2..].find("*/") else {
                break;
            };
            index += end + 4;
        } else if matches!(byte, b'\'' | b'"') {
            let Some(value) = string(script, &mut index) else {
                break;
            };
            tokens.push(Token::Literal(Value::Text(value)));
        } else if byte == 0x60 {
            // Template expressions are not static string assignments. Skip the entire literal.
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if bytes[index] == 0x60 {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
            tokens.push(Token::Other);
        } else if byte == b'/'
            && matches!(
                tokens.last(),
                None | Some(Token::Punctuation(b'=' | b'(' | b',' | b':' | b';' | b'['))
            )
        {
            index += 1;
            let mut character_class = false;
            while index < bytes.len() {
                match bytes[index] {
                    b'\\' => {
                        index = (index + 2).min(bytes.len());
                        continue;
                    }
                    b'[' => character_class = true,
                    b']' => character_class = false,
                    b'/' if !character_class => {
                        index += 1;
                        break;
                    }
                    b'\n' | b'\r' => break,
                    _ => {}
                }
                index += 1;
            }
            while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
                index += 1;
            }
            tokens.push(Token::Other);
        } else if byte.is_ascii_digit() {
            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            tokens.push(
                script[start..index]
                    .parse()
                    .map(|n| Token::Literal(Value::Number(n)))
                    .unwrap_or(Token::Other),
            );
        } else if identifier(byte) {
            let start = index;
            while index < bytes.len() && identifier(bytes[index]) {
                index += 1;
            }
            tokens.push(Token::Identifier(script[start..index].to_owned()));
        } else if byte.is_ascii() {
            tokens.push(Token::Punctuation(byte));
            index += 1;
        } else {
            index += script[index..].chars().next().unwrap().len_utf8();
            tokens.push(Token::Other);
        }
    }
    tokens
}

pub(super) fn values(html: &str) -> HashMap<String, Value> {
    let mut values = HashMap::new();
    for script in document::parse(html).scripts {
        let tokens = tokens(script);
        let mut depth: usize = 0;
        for (index, token) in tokens.iter().enumerate() {
            match token {
                Token::Punctuation(b'{' | b'[' | b'(') => {
                    depth += 1;
                    continue;
                }
                Token::Punctuation(b'}' | b']' | b')') => {
                    depth = depth.saturating_sub(1);
                    continue;
                }
                _ => {}
            }
            if depth != 0 {
                continue;
            }
            let Token::Identifier(name) = token else {
                continue;
            };
            let boundary = match index.checked_sub(1).and_then(|i| tokens.get(i)) {
                None | Some(Token::Punctuation(b';' | b',')) => true,
                Some(Token::Identifier(keyword)) => {
                    matches!(keyword.as_str(), "var" | "let" | "const")
                }
                _ => false,
            };
            if !boundary || !matches!(tokens.get(index + 1), Some(Token::Punctuation(b'='))) {
                continue;
            }
            if !matches!(
                tokens.get(index + 3),
                None | Some(Token::Punctuation(b';' | b','))
            ) {
                continue;
            }
            let value = match tokens.get(index + 2) {
                Some(Token::Literal(Value::Text(text))) => Value::Text(text.clone()),
                Some(Token::Literal(Value::Number(number))) => Value::Number(*number),
                _ => continue,
            };
            values.insert(name.clone(), value);
        }
    }
    values
}

fn expression_end(tokens: &[Token], start: usize) -> Option<usize> {
    let mut stack = Vec::new();
    for (index, token) in tokens.iter().enumerate().skip(start) {
        match token {
            Token::Punctuation(b',' | b'}') if stack.is_empty() => return Some(index),
            Token::Punctuation(open @ (b'{' | b'[' | b'(')) => stack.push(*open),
            Token::Punctuation(close @ (b'}' | b']' | b')')) => {
                let expected = match close {
                    b'}' => b'{',
                    b']' => b'[',
                    _ => b'(',
                };
                if stack.pop() != Some(expected) {
                    return None;
                }
            }
            _ => {}
        }
    }
    None
}

fn object_literals(
    tokens: &[Token],
    start: usize,
    prefix: &str,
    output: &mut HashMap<String, Value>,
    depth: usize,
) -> Option<usize> {
    if depth > 8 || !matches!(tokens.get(start), Some(Token::Punctuation(b'{'))) {
        return None;
    }
    let mut index = start + 1;
    loop {
        if matches!(tokens.get(index), Some(Token::Punctuation(b'}'))) {
            return Some(index + 1);
        }
        let key = match tokens.get(index)? {
            Token::Identifier(key) | Token::Literal(Value::Text(key)) => key,
            _ => return None,
        };
        let key = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        if !matches!(tokens.get(index + 1), Some(Token::Punctuation(b':'))) {
            return None;
        }
        let start = index + 2;
        let end = expression_end(tokens, start)?;
        // A later duplicate/computed value must not leave an earlier trusted literal behind.
        output.retain(|existing, _| existing != &key && !existing.starts_with(&format!("{key}.")));
        if end == start + 1 {
            let value = match &tokens[start] {
                Token::Literal(value) => Some(value.clone()),
                Token::Identifier(value) if value == "true" => Some(Value::Bool(true)),
                Token::Identifier(value) if value == "false" => Some(Value::Bool(false)),
                _ => None,
            };
            if let Some(value) = value {
                output.insert(key, value);
            }
        } else if matches!(tokens.get(start), Some(Token::Punctuation(b'{'))) {
            let mut nested = HashMap::new();
            if object_literals(tokens, start, &key, &mut nested, depth + 1) == Some(end) {
                output.extend(nested);
            }
        }
        index = end;
        if matches!(tokens.get(index), Some(Token::Punctuation(b','))) {
            index += 1;
        }
    }
}

/// Extract literal properties only. Expressions (including lang's ||), calls and templates are never executed.
pub(super) fn object_values(html: &str, wanted: &str) -> Option<HashMap<String, Value>> {
    let mut found = None;
    for script in document::parse(html).scripts {
        let tokens = tokens(script);
        let mut depth: usize = 0;
        for (index, token) in tokens.iter().enumerate() {
            match token {
                Token::Punctuation(b'{' | b'[' | b'(') => {
                    depth += 1;
                    continue;
                }
                Token::Punctuation(b'}' | b']' | b')') => {
                    depth = depth.saturating_sub(1);
                    continue;
                }
                _ => {}
            }
            if depth != 0 || !matches!(token, Token::Identifier(name) if name == wanted) {
                continue;
            }
            let boundary = match index.checked_sub(1).and_then(|i| tokens.get(i)) {
                None | Some(Token::Punctuation(b';' | b',')) => true,
                Some(Token::Identifier(word)) => matches!(word.as_str(), "var" | "let" | "const"),
                _ => false,
            };
            if !boundary || !matches!(tokens.get(index + 1), Some(Token::Punctuation(b'='))) {
                continue;
            }
            found = None;
            let mut values = HashMap::new();
            if let Some(end) = object_literals(&tokens, index + 2, "", &mut values, 0) {
                if matches!(
                    tokens.get(end),
                    None | Some(Token::Punctuation(b';' | b','))
                ) {
                    found = Some(values);
                }
            }
        }
    }
    found
}
