pub mod api;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod pdf_guard;
pub mod printers;
pub mod worker;

pub const PRODUCT_NAME: &str = "PrintLatch";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_PORT: u16 = 32_191;
pub const MAX_JOB_BYTES: usize = 10 * 1024 * 1024;
