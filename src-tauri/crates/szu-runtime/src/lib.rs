//! 本地模型运行时。
//!
//! `local_model` 负责模型文件的下载、校验与目录管理；`ocr` 与
//! `embedding_model` 在它之上提供验证码识别与文本向量两种能力。

pub mod embedding_model;
pub mod local_model;
pub mod ocr;
