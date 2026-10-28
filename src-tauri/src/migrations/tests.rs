use std::collections::BTreeMap;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn oj_persists_notice_read_state_and_group_cache() {
    use crate::oj::model::{Group, GroupCache, Notice, Site};

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("oj.sqlite");
    let db = crate::oj::db::OjDatabase::open(&db_path).await.unwrap();

    let site = Site {
        id: "site".into(),
        name: "测试 OJ".into(),
        url: "http://oj.test/".into(),
        builtin: false,
        reminders: true,
    };
    let mut data = crate::oj::model::Data {
        sites: vec![site],
        ..Default::default()
    };
    data.notices.push(Notice {
        id: "n1".into(),
        site_id: "site".into(),
        account: "student".into(),
        contest_id: "1".into(),
        title: "标题".into(),
        message: "内容".into(),
        timestamp: 1,
        read: false,
    });
    data.groups.insert(
        "site".into(),
        GroupCache {
            groups: vec![Group {
                id: "2".into(),
                name: "一班".into(),
            }],
            updated_at: 123,
        },
    );
    db.save_data(&data).await.unwrap();

    // 已读状态必须写回数据库，而不是只在内存里生效。
    data.notices[0].read = true;
    db.save_data(&data).await.unwrap();
    let loaded = db.load_data().await.unwrap();
    assert!(loaded.notices[0].read);
    assert_eq!(loaded.groups["site"].groups[0].name, "一班");

    // 站点删除后，其通知不应在下次加载时“复活”。
    data.sites.clear();
    db.save_data(&data).await.unwrap();
    let loaded = db.load_data().await.unwrap();
    assert!(loaded.notices.is_empty());
}

#[tokio::test]
async fn test_oj_state_migration() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("oj.sqlite");
    let json_path = dir.path().join("oj.json");

    let initial_data = crate::oj::model::Data {
        version: 1,
        sites: vec![],
        drafts: BTreeMap::new(),
        known: BTreeMap::new(),
        notices: vec![],
        uncertain: BTreeMap::new(),
        scheduled: BTreeMap::new(),
        companion_port: 8080,
        preferences: BTreeMap::new(),
        companion_enabled: true,
        groups: BTreeMap::new(),
        contests: BTreeMap::new(),
    };

    fs::write(&json_path, serde_json::to_vec(&initial_data).unwrap()).unwrap();
    assert!(json_path.exists());

    let db = crate::oj::db::OjDatabase::open(&db_path).await.unwrap();
    let migrated = super::oj::migrate_oj_state_if_needed(&db_path, &db)
        .await
        .unwrap();

    assert!(migrated.is_some());
    assert_eq!(migrated.unwrap().companion_port, 8080);
    // 验证旧文件已自动清理
    assert!(!json_path.exists());

    // 验证数据库可直接读取
    let loaded = db.load_data().await.unwrap();
    assert_eq!(loaded.companion_port, 8080);
}

#[tokio::test]
async fn test_electricity_history_migration() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("electricity.sqlite");
    let json_path = dir.path().join("electricity-history-v1.json");

    #[derive(serde::Serialize)]
    struct LegacyDiskCache {
        version: u8,
        rooms: Vec<crate::campus_payments::electricity::history::Room>,
    }

    let mut room = crate::campus_payments::electricity::history::Room::default();
    room.location.area_id = "area_a".into();
    room.location.building_id = "b1".into();
    room.location.room_name = "101".into();
    room.balance = Some(88.5);

    let disk = LegacyDiskCache {
        version: 1,
        rooms: vec![room],
    };
    fs::write(&json_path, serde_json::to_vec(&disk).unwrap()).unwrap();
    assert!(json_path.exists());

    let db = crate::campus_payments::electricity::db::ElectricityDatabase::open(&db_path)
        .await
        .unwrap();
    let migrated = super::electricity::migrate_electricity_history_if_needed(&db_path, &db)
        .await
        .unwrap();

    assert_eq!(migrated.len(), 1);
    assert_eq!(migrated[0].balance, Some(88.5));
    // 验证旧文件已删除
    assert!(!json_path.exists());

    // 验证数据库中能读取到
    let all_rooms = db.load_all_rooms().await.unwrap();
    assert_eq!(all_rooms.len(), 1);
    assert_eq!(all_rooms[0].location.room_name, "101");
}

#[tokio::test]
async fn test_uooc_video_duration_migration() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("quiz_qa_bank.db");
    let json_path = dir.path().join("video-durations.json");

    let mut map = std::collections::HashMap::new();
    map.insert("cid_1:res_1".to_string(), 125.5);
    fs::write(&json_path, serde_json::to_vec(&map).unwrap()).unwrap();
    assert!(json_path.exists());

    let db = crate::uooc::quiz::db::QaDatabase::open(&db_path).await.unwrap();
    let migrated = super::uooc::migrate_video_durations_if_needed(&db_path, &db)
        .await
        .unwrap();

    assert_eq!(migrated.get("cid_1:res_1"), Some(&125.5));
    assert!(!json_path.exists());

    let loaded = db.load_all_video_durations().await.unwrap();
    assert_eq!(loaded.get("cid_1:res_1"), Some(&125.5));
}

#[test]
fn test_uooc_legacy_files_cleanup() {
    let dir = tempdir().unwrap();
    let p1 = dir.path().join("qa-bank.json");
    let p2 = dir.path().join("qa-vector-cache.json");
    fs::write(&p1, b"{}").unwrap();
    fs::write(&p2, b"{}").unwrap();

    assert!(p1.exists());
    assert!(p2.exists());

    super::uooc::cleanup_legacy_uooc_files(dir.path());

    assert!(!p1.exists());
    assert!(!p2.exists());
}
