//! Independent, read-only account client for Mahjong Soul.
mod client;
mod discovery;
mod metadata;
mod policy;
mod protocol;

pub use client::derive_password;
pub use client::{Client, Snapshot};
pub use discovery::{Discovery, discover, invalidate_discovery};
pub use policy::Policy;
pub use zeroize::Zeroizing;

/// The Unity package used to regenerate the bundled protocol definitions.
pub const PROTOCOL_VERSION: &str = "4.0.51";

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{method} 返回错误码 {code}：{hint}", hint = server_error_hint(*code))]
    Server { method: String, code: u64 },
    #[error("请求过于频繁，请在 {seconds} 秒后重试")]
    Cooldown { seconds: u64 },
    #[error("连接已中断，请重新登录；已有账号数据已保留")]
    Disconnected,
    #[error("服务器正在维护，请稍后手动重试")]
    Maintenance,
}

fn server_error_hint(code: u64) -> &'static str {
    // Local Unity info.error -> info_error_chs, checked against the client tables.
    match code {
        1002 => "账号不存在，请检查登录账号及邮箱后缀的大小写",
        1003 => "密码错误，请检查密码或通过游戏重置密码",
        _ => "请确认账号、客户端版本或服务状态后手动重试",
    }
}
