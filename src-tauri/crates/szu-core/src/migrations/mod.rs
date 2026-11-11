//! 核心域自己的数据库与旧版数据迁移。
//!
//! 这里的 [`db`] 是全体功能模块共用的 SQLite 句柄工厂；其余模块处理偏好设置、
//! 凭据、向导与日志/模型的旧位置搬迁。各功能模块自己的表结构迁移跟随模块
//! 放在各自的 crate 里。

pub mod db;
pub mod electricity;
pub mod keyring;
pub mod onboarding;
pub mod preferences;
pub mod storage;
