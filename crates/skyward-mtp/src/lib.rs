//! Media Transfer Protocol: the phone's internal storage on a computer over
//! USB, the way the stock system offers it.

pub mod codes;
pub mod dataset;
pub mod format;
pub mod responder;
pub mod store;
pub mod time;

#[cfg(target_os = "android")]
mod service;
#[cfg(target_os = "android")]
mod usb;

pub use responder::{Device, Storage};
#[cfg(target_os = "android")]
pub use service::Service;
