use std::collections::BTreeMap;
use std::fs;
use tempfile::tempdir;

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
