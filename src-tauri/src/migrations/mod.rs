//! 功能模块自己的数据库迁移。
//!
//! 通用的 SQLite 句柄工厂在 [`szu_core::migrations::db`]，这里重导出以便
//! 尚未搬走的模块继续使用 `crate::migrations::db`；功能模块下沉到各自 crate
//! 后，迁移文件会跟随模块搬走。

pub use szu_core::migrations::db;

pub mod enrollment;
pub mod mesh;
pub mod todos;

#[cfg(test)]
mod tests;
