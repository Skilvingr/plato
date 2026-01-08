mod fake;
mod kobo;

use anyhow::Error;

pub use self::fake::FakeBattery;
pub use self::kobo::KoboBattery;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Status {
    Discharging,
    Charging,
    Charged,
    Unknown, // Full,
}

impl From<&str> for Status {
    fn from(value: &str) -> Self {
        if value.starts_with("Discharging") {
            Self::Discharging
        } else if value.starts_with("Charging") {
            Self::Charging
        } else if value.starts_with("Charged")
            || value.starts_with("Not Charging")
            || value.starts_with("Full")
        {
            Self::Charged
        } else {
            Self::Unknown
        }
    }
}

impl Status {
    pub fn is_wired(self) -> bool {
        matches!(self, Status::Charging | Status::Charged)
    }
}

pub trait Battery {
    fn capacity(&mut self) -> Result<Vec<u8>, Error>;
    fn status(&mut self) -> Result<Vec<Status>, Error>;
    fn charge_full(&self) -> Result<f32, Error>;
    fn charge_full_design(&self) -> Result<f32, Error>;
}
