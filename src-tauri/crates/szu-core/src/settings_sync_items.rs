//! 设备同步项的稳定 id。
//!
//! 业务模块与 `szu-mesh` 共用这一组常量，避免在保存点与同步白名单之间出现
//! 字面量漂移。

pub const UOOC_SESSION: &str = "uooc.session";
pub const UOOC_AI: &str = "uooc.ai";
pub const OJ_ACCOUNTS: &str = "oj.accounts";
pub const DORM_CREDENTIALS: &str = "network.dorm.credentials";
pub const STAFF_CREDENTIALS: &str = "network.staff.credentials";
pub const ELECTRICITY_LOCATION: &str = "electricity.location";
pub const UOOC_SETTINGS: &str = "uooc.settings";
pub const UOOC_QA_BANK: &str = "uooc.qa_bank";
