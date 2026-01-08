use std::{
    fs::File,
    io::{self, Read, Seek},
};

use super::{Battery, Status};
use anyhow::Error;

pub struct FakeBattery {
    battery_cap_path: File,
    battery_status_path: File,
}

impl FakeBattery {
    pub fn new(battery_sysfs: &str) -> io::Result<FakeBattery> {
        Ok(FakeBattery {
            battery_cap_path: File::open(format!("{}/capacity", battery_sysfs))?,
            battery_status_path: File::open(format!("{}/status", battery_sysfs))?,
        })
    }
}

impl Battery for FakeBattery {
    fn capacity(&mut self) -> Result<Vec<u8>, Error> {
        self.battery_cap_path.rewind()?;
        let mut buf = String::new();
        self.battery_cap_path.read_to_string(&mut buf)?;
        let c = buf.trim_end().parse::<f32>().unwrap_or(100.) as u8;

        Ok(vec![c])
    }

    fn status(&mut self) -> Result<Vec<Status>, Error> {
        self.battery_status_path.rewind()?;
        let mut buf = String::new();
        self.battery_status_path.read_to_string(&mut buf)?;

        Ok(vec![Status::from(buf.as_str())])
    }

    fn charge_full(&self) -> Result<f32, Error> {
        Ok(1900.0)
    }

    fn charge_full_design(&self) -> Result<f32, Error> {
        Ok(2000.)
    }
}
