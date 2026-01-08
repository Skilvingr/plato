#[macro_use]
pub mod geom;

pub mod battery;
pub mod colour;
pub mod context;
pub mod device;
mod dictionary;
pub mod document;
pub mod font;
pub mod framebuffer;
pub mod frontlight;
pub mod helpers;
pub mod input;
pub mod library;
pub mod lightsensor;
pub mod metadata;
pub mod peripherals;
pub mod rtc;
pub mod settings;
pub mod ssh;
pub mod tasks;
mod unit;
pub mod view;

pub use anyhow;
pub use chrono;
pub use fxhash;
pub use globset;
pub use png;
pub use rand_core;
pub use rand_xoshiro;
pub use serde;
pub use serde_json;
pub use walkdir;

#[derive(PartialEq)]
pub enum ExitStatus {
    Quit,
    RestartApp,
    Reboot,
    PowerOff,
}
