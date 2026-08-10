//! routing v2 的宿主适配层。
//!
//! 此目录逐步承载 SQLite、Vault 和协议等宿主实现；它不得反向污染旧 Proxy
//! 发送链路，且 routing-core 始终保持为独立的纯 Rust crate。

pub(crate) mod routing_store;
