//! 各功能模块的 MCP 工具胶水。
//!
//! 工具实现挂在 [`crate::mcp::service::UnifiedSzuMcpService`] 的固有 impl 上，
//! 而 rmcp 的 `ToolRouter<H>` 由处理者类型决定，因此全部工具必须与
//! `UnifiedSzuMcpService` 处在同一个 crate。这里集中放置它们，功能模块本身
//! 因此不再依赖 `mcp`，可以独立成 crate。

pub(crate) mod app_content;
pub(crate) mod appearance;
pub(crate) mod auth;
pub(crate) mod campus_network;
pub(crate) mod campus_payments;
pub(crate) mod embedding_model;
pub(crate) mod enrollment;
pub(crate) mod file_manager;
pub(crate) mod jxxk;
pub(crate) mod lms;
pub(crate) mod mesh;
pub(crate) mod signin;
pub(crate) mod netproxy;
pub(crate) mod notification;
pub(crate) mod ocr;
pub(crate) mod oj;
pub(crate) mod onboarding;
pub(crate) mod settings;
pub(crate) mod settings_sync;
pub(crate) mod todos;
pub(crate) mod uooc;
