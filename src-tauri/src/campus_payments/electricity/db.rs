use super::history::*;
use super::*;
use std::path::Path;

#[derive(Debug, toasty::Model)]
pub struct ElectricityRoomRecord {
    #[key]
    pub id: String, // format!("{area}:{building}:{room}")
    pub location_json: String,
    pub readings_json: String,
    pub recharges_json: String,
    pub usage_checks_json: String,
    pub recharge_checks_json: String,
    pub balance: Option<f64>,
    pub balance_date: Option<String>,
    pub usage_updated_at: Option<i64>,
    pub recharge_updated_at: Option<i64>,
    pub retry_after: i64,
    pub error: Option<String>,
    pub recharge_error: Option<String>,
    pub recharge_retry_after: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone)]
pub(crate) struct ElectricityDatabase {
    pub(crate) db: toasty::Db,
}

impl ElectricityDatabase {
    pub async fn open(db_path: &Path) -> Result<Self, String> {
        let db = crate::migrations::db::open_sqlite_db(
            db_path,
            toasty::models!(ElectricityRoomRecord),
            Some(&crate::migrations::electricity::ELECTRICITY_MIGRATIONS),
        )
        .await?;
        Ok(Self { db })
    }

    pub async fn load_all_rooms(&self) -> Result<Vec<Room>, String> {
        let mut db = self.db.clone();
        let rows = match ElectricityRoomRecord::all().exec(&mut db).await {
            Ok(rows) => rows,
            Err(_) => return Ok(vec![]),
        };

        let mut rooms = Vec::new();
        for r in rows {
            let location: ElectricityLocationSettings =
                serde_json::from_str(&r.location_json).unwrap_or_default();
            let readings: Vec<ElectricityReading> =
                serde_json::from_str(&r.readings_json).unwrap_or_default();
            let loaded_recharges: Vec<Recharge> =
                serde_json::from_str(&r.recharges_json).unwrap_or_default();
            let usage_checks: BTreeMap<String, Check> =
                serde_json::from_str(&r.usage_checks_json).unwrap_or_default();
            let recharge_checks: BTreeMap<String, Check> =
                serde_json::from_str(&r.recharge_checks_json).unwrap_or_default();

            rooms.push(Room {
                location,
                readings,
                recharges: loaded_recharges,
                usage_checks,
                recharge_checks,
                balance: r.balance,
                balance_date: r.balance_date,
                usage_updated_at: r.usage_updated_at,
                recharge_updated_at: r.recharge_updated_at,
                retry_after: r.retry_after,
                error: r.error,
                recharge_error: r.recharge_error,
                recharge_retry_after: r.recharge_retry_after,
            });
        }
        Ok(rooms)
    }

    pub async fn save_room(&self, room: &Room) -> Result<(), String> {
        let key = format!(
            "{}:{}:{}",
            room.location.area_id, room.location.building_id, room.location.room_name
        );
        let location_json = serde_json::to_string(&room.location).unwrap_or_default();
        let readings_json = serde_json::to_string(&room.readings).unwrap_or_default();
        let recharges_json = serde_json::to_string(&room.recharges).unwrap_or_default();
        let usage_checks_json = serde_json::to_string(&room.usage_checks).unwrap_or_default();
        let recharge_checks_json = serde_json::to_string(&room.recharge_checks).unwrap_or_default();
        let now = chrono::Utc::now().timestamp_millis();

        let mut db = self.db.clone();
        if let Ok(mut existing) = ElectricityRoomRecord::get_by_id(&mut db, &key).await {
            toasty::update!(existing {
                location_json,
                readings_json,
                recharges_json,
                usage_checks_json,
                recharge_checks_json,
                balance: room.balance,
                balance_date: room.balance_date.clone(),
                usage_updated_at: room.usage_updated_at,
                recharge_updated_at: room.recharge_updated_at,
                retry_after: room.retry_after,
                error: room.error.clone(),
                recharge_error: room.recharge_error.clone(),
                recharge_retry_after: room.recharge_retry_after,
                updated_at_ms: now,
            })
            .exec(&mut db)
            .await
            .map_err(|e| format!("更新电量记录失败: {e}"))?;
        } else {
            toasty::create!(ElectricityRoomRecord {
                id: key,
                location_json,
                readings_json,
                recharges_json,
                usage_checks_json,
                recharge_checks_json,
                balance: room.balance,
                balance_date: room.balance_date.clone(),
                usage_updated_at: room.usage_updated_at,
                recharge_updated_at: room.recharge_updated_at,
                retry_after: room.retry_after,
                error: room.error.clone(),
                recharge_error: room.recharge_error.clone(),
                recharge_retry_after: room.recharge_retry_after,
                updated_at_ms: now,
            })
            .exec(&mut db)
            .await
            .map_err(|e| format!("新增电量记录失败: {e}"))?;
        }

        Ok(())
    }
}
