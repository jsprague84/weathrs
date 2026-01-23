pub mod handlers;
pub mod models;
mod service;
mod storage;

pub use models::{Device, Platform};
pub use service::DevicesService;
