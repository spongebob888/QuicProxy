//! API 模块 — 核心监控 API + 管理 API + 持久化 API + 反向代理 + 静态文件服务
//!
//! ## 子模块
//! - `common`           — CORS 中间件、认证（共享）
//! - `core_manager`     — 核心进程管理器（启动/停止/重启/日志）
//! - `persist_store`    — 持久化存储（内存 HashMap + 文件）
//! - `core_api`         — 实时监控代理状态、切换节点/模式（core 进程内）
//! - `management`       — 核心生命周期管理 API handlers
//! - `persist_handler`  — 持久化存储 CRUD API handlers
//! - `reverse_proxy`    — 反向代理到子核心进程的 API
//! - `static_files`     — SPA fallback 静态文件服务

pub mod common;
pub mod core_api;
pub mod core_manager;
pub mod management;
pub mod persist_handler;
pub mod persist_store;
pub mod reverse_proxy;
pub mod static_files;

// 保持历史兼容：bootstrap.rs 使用 `crate::api::init_api`
pub use core_api::init_core_api as init_api;

// selector.rs 使用 `crate::api::get_outbound_info`
pub use core_api::get_outbound_info;
pub use core_api::TraceResponse;
