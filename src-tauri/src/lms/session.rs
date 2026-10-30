use super::model::Profile;

/// 从畅课页面 HTML 中解析注入的 `globalData` 配置脚本，提取当前用户资料。
///
/// 页面会注入如下结构：
/// ```html
/// <script>
///     var globalData = {
///         user: { id: 12345, name: "张三", userNo: "2020010000", ... },
///         ...
///     }
/// </script>
/// ```
/// `user.id` 是通知接口路径里的 uid。
pub(super) fn parse_global_data(html: &str) -> Option<Profile> {
    let marker = "var globalData";
    let start = html.find(marker)? + marker.len();
    let open = html[start..].find('{')? + start;
    let object = balanced_object(&html[open..])?;
    // globalData 是 JS 对象字面量，键名不带引号，先补上引号再交给 serde。
    let json = quote_object_keys(object);
    let value: serde_json::Value = serde_json::from_str(&json).ok()?;
    let user = &value["user"];
    Some(Profile {
        uid: user["id"].as_i64()?,
        name: user["name"].as_str().unwrap_or_default().to_owned(),
        user_no: user["userNo"].as_str().unwrap_or_default().to_owned(),
    })
}

/// 把 JS 对象字面量里不带引号的键名补上引号、去掉 `}` / `]` 前的尾逗号，
/// 转成可被 serde_json 解析的文本。字符串字面量内部不做改写。
fn quote_object_keys(source: &str) -> String {
    let mut out = String::with_capacity(source.len() + 16);
    let mut in_string = false;
    let mut escaped = false;
    let mut pending_key = String::new();
    let mut has_pending_key = false;
    for ch in source.chars() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => {
                if has_pending_key {
                    // 键名本身是带引号的字符串，原样保留。
                    out.push_str(&pending_key);
                    pending_key.clear();
                    has_pending_key = false;
                }
                in_string = true;
                out.push(ch);
            }
            ':' => {
                if has_pending_key && !pending_key.trim().is_empty() {
                    out.push('"');
                    out.push_str(pending_key.trim());
                    out.push('"');
                    pending_key.clear();
                    has_pending_key = false;
                }
                out.push(ch);
            }
            '}' | ']' => {
                // JS 允许尾逗号（`false, }`），JSON 不允许：去掉闭括号前的
                // 尾逗号与空白。
                while out
                    .chars()
                    .last()
                    .is_some_and(|last| last.is_whitespace() || last == ',')
                {
                    out.pop();
                }
                out.push(ch);
            }
            c if c.is_ascii_alphanumeric() || c == '_' || c == '$' => {
                if has_pending_key {
                    pending_key.push(c);
                } else if c.is_ascii_alphabetic() || c == '_' || c == '$' {
                    pending_key.push(c);
                    has_pending_key = true;
                } else {
                    // 数字等值开头，直接透传。
                    out.push(c);
                }
            }
            _ => {
                if has_pending_key {
                    // 非键名片段（如 `123`、`true`），原样透传。
                    out.push_str(&pending_key);
                    pending_key.clear();
                    has_pending_key = false;
                }
                out.push(ch);
            }
        }
    }
    if has_pending_key {
        out.push_str(&pending_key);
    }
    out
}

/// 从 `{` 开始扫描一个括号配平的 JSON 对象，返回其文本切片。
/// 字符串字面量里的花括号与转义不参与配平。
fn balanced_object(source: &str) -> Option<&str> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, byte) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(&source[..=index]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_global_data_from_injected_script() {
        // 真实页面带尾逗号（`hasAirService: false,`），dept.id 是字符串。
        let html = r#"<html><head><title>荔园畅课</title></head><body>
        <script>
            var globalData = {
                user: {
                    id: 12345,
                    name: "张三",
                    userNo: "2020010000",
                    orgId: 1,
                    mobile: "None",
                    orgName: "荔园畅课",
                    role: "Student",
                    hasAiAbility: false,
                    hasAirService: false,
                },
                dept: { id: "56", name: "xx学院", code: "02011000" },
                lang: "zh-CN",
            }
        </script>
        </body></html>"#;
        let profile = parse_global_data(html).expect("profile should parse");
        assert_eq!(profile.uid, 12345);
        assert_eq!(profile.name, "张三");
        assert_eq!(profile.user_no, "2020010000");
    }

    #[test]
    fn tolerates_braces_inside_strings_and_reports_missing_user() {
        let html = r#"<script>var globalData = { user: { id: 7, name: "a}b\"{c", orgCode: "szu", } };</script>"#;
        let profile = parse_global_data(html).expect("profile should parse");
        assert_eq!(profile.uid, 7);
        assert_eq!(profile.name, "a}b\"{c");
        assert!(parse_global_data("<p>no config</p>").is_none());
    }
}
