pub mod handlers;
pub mod models;
mod service;

pub use models::{Device, Platform};
pub use service::{DevicesError, DevicesService};
