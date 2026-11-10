use super::{todo_menu_label, todo_menu_specs, TODO_MENU_LIMIT};
use crate::todos::model::{TodoItem, TodoKind};

fn item(title: &str, due_at_ms: i64) -> TodoItem {
    TodoItem {
        id: format!("id-{title}"),
        kind: TodoKind::OjQuiz,
        title: title.to_string(),
        subtitle: None,
        due_at_ms,
        route: "/study/oj".into(),
        external_url: None,
        urgent: false,
    }
}

#[test]
fn empty_items_show_disabled_placeholder() {
    let specs = todo_menu_specs(&[]);
    assert_eq!(
        specs,
        vec![
            ("show".to_string(), "打开窗口".to_string(), true),
            ("todos-empty".to_string(), "暂无待办".to_string(), false),
            ("exit".to_string(), "退出".to_string(), true),
        ]
    );
}

#[test]
fn todos_are_listed_with_indexed_ids_and_capped() {
    // 输入已按截止时间升序（aggregate 的排序结果），截断时保留靠前的条目。
    let items: Vec<TodoItem> = (0..TODO_MENU_LIMIT + 5)
        .map(|index| item(&format!("待办{index}"), 1_700_000_000_000 + index as i64))
        .collect();
    let specs = todo_menu_specs(&items);

    let todo_specs: Vec<_> = specs
        .iter()
        .filter(|(id, ..)| id.starts_with("todo:"))
        .collect();
    assert_eq!(todo_specs.len(), TODO_MENU_LIMIT);
    for (index, (id, label, enabled)) in todo_specs.iter().enumerate() {
        assert_eq!(id, &format!("todo:{index}"));
        assert!(enabled, "待办菜单项应可用");
        assert!(label.contains(&format!("待办{index}")), "顺序应与输入一致");
    }
    assert_eq!(
        specs.last(),
        Some(&("exit".to_string(), "退出".to_string(), true)),
    );
}

#[test]
fn label_truncates_long_titles_with_ellipsis() {
    let long = "这是一个特别特别特别长的待办标题超过了二十个字符的界限";
    let item = item(long, 1_700_000_000_000);
    let label = todo_menu_label(&item);
    assert!(!label.contains(long));
    // 截断后的标题部分 ≤ 20 个字符 + 省略号，后接（MM-dd 截止）。
    let title_part = label.split('（').next().unwrap();
    assert!(title_part.ends_with('…'), "实际标题部分: {title_part}");
    assert_eq!(title_part.chars().count(), 21);
}

#[test]
fn label_formats_due_date_as_month_day() {
    // 2023-11-14 22:13 UTC+8 → 本地时间格式化为 MM-dd；用固定时区差验证年月日部分。
    let item = item("OJ 测验", 1_700_000_000_000);
    let label = todo_menu_label(&item);
    assert!(label.starts_with("OJ 测验（"), "实际文案: {label}");
    assert!(label.ends_with("截止）"));
    let date = label
        .trim_start_matches("OJ 测验（")
        .trim_end_matches(" 截止）");
    assert_eq!(date.len(), 5, "MM-dd 格式: {date}");
}

#[test]
fn label_falls_back_to_title_when_due_date_invalid() {
    let item = item("电费预警", i64::MAX / 2);
    assert_eq!(todo_menu_label(&item), "电费预警");
}
