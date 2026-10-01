//! VRChat API (api.vrchat.cloud) の薄いクライアント。
//!
//! このツールが使うエンドポイントだけを実装する。API は非公式であり予告なく変わり得る。

pub mod client;
pub mod error;
pub mod models;
pub mod retry;

pub use client::{AuthResponse, VrcClient};
pub use error::{Error, Result};
pub use models::*;
