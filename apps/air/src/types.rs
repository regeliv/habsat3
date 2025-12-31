use std::time::Duration;

use system_sensors::{FilesystemUsageInfo, MemoryUsageInfo};
use uom::si::f64::ThermodynamicTemperature;

use crate::db::models::{
    NewBnoReading, NewCpuTemperature, NewFsUsage, NewMemoryUsage, NewTel0157Reading,
};

#[derive(Debug, Clone, Copy)]
pub struct Tick {
    pub unix_time: Duration,
}

impl Tick {
    pub fn as_secs(&self) -> f64 {
        self.unix_time.as_secs_f64()
    }
}

#[derive(Debug, Clone)]
pub struct Timestamped<T: Clone + std::fmt::Debug> {
    pub timestamp: Tick,
    pub data: T,
}

impl<T: Clone + std::fmt::Debug> Timestamped<T> {
    pub fn new(timestamp: Tick, data: T) -> Self {
        Self { timestamp, data }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DataBatches {
    pub mem_usages: Vec<NewMemoryUsage>,
    pub cpu_temps: Vec<NewCpuTemperature>,
    pub fs_usages: Vec<NewFsUsage>,
    pub bno_readings: Vec<NewBnoReading>,
    pub tel0157_readings: Vec<NewTel0157Reading>,
}

impl DataBatches {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.mem_usages.clear();
        self.cpu_temps.clear();
        self.fs_usages.clear();
        self.bno_readings.clear();
    }

    pub fn total_len(&self) -> usize {
        self.mem_usages.len()
            + self.cpu_temps.len()
            + self.fs_usages.len()
            + self.bno_readings.len()
    }
}

pub struct RxDataChannels {
    pub mem_usage: kanal::AsyncReceiver<Timestamped<MemoryUsageInfo>>,
    pub cpu_temp: kanal::AsyncReceiver<Timestamped<ThermodynamicTemperature>>,
    pub fs_usage: kanal::AsyncReceiver<Timestamped<FilesystemUsageInfo>>,
    pub bno_reading: kanal::AsyncReceiver<Timestamped<bno_055::SensorData>>,
    pub tel0157_reading: kanal::AsyncReceiver<Timestamped<tel0157::Tel0157Reading>>,
}
