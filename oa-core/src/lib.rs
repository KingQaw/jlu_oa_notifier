//! 吉林大学电子校务平台（校内通知）核心库。
//!
//! 分层设计：
//! - `parse`：纯 Rust 的 HTML 解析（无网络/系统依赖），可独立编译测试。
//! - `fetch`：基于 reqwest 的抓取层（默认 `http` 特性启用）。

pub mod models;
pub mod parse;

#[cfg(feature = "http")]
pub mod fetch;

pub use models::{Access, Attachment, ListOptions, ListResult, NoticeDetail, NoticeItem};
pub use parse::{parse_detail, parse_list, parse_orgs};

#[cfg(feature = "http")]
pub use fetch::{
    build_attachment_url, fetch_detail, fetch_image, fetch_list, search_orgs, BASE, CHANNEL_ID,
    NEED_LOGIN, NETWORK_UNREACHABLE,
};
