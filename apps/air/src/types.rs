use std::time::Duration;
use system_sensors::{FilesystemUsageInfo, MemoryUsageInfo};
use uom::si::f64::ThermodynamicTemperature;

use crate::db::models::{
    NewAs7341Reading, NewBmp280Reading, NewBnoReading, NewCpuTemperature, NewFsUsage,
    NewMemoryUsage, NewTel0157Reading,
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

#[derive(Debug, Clone)]
pub struct Labeled<T: Clone + std::fmt::Debug> {
    pub label: u8,
    pub data: T,
}

impl<T: Clone + std::fmt::Debug> Labeled<T> {
    pub fn new(label: u8, data: T) -> Self {
        Self { label, data }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DataBatches {
    pub mem_usages: Vec<NewMemoryUsage>,
    pub cpu_temps: Vec<NewCpuTemperature>,
    pub fs_usages: Vec<NewFsUsage>,
    pub bno_readings: Vec<NewBnoReading>,
    pub tel0157_readings: Vec<NewTel0157Reading>,
    pub bmp280_readings: Vec<NewBmp280Reading>,
    pub as7341_readings: Vec<NewAs7341Reading>,
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
        self.tel0157_readings.clear();
        self.bmp280_readings.clear();
    }

    pub fn total_len(&self) -> usize {
        self.mem_usages.len()
            + self.cpu_temps.len()
            + self.fs_usages.len()
            + self.bno_readings.len()
            + self.tel0157_readings.len()
            + self.bmp280_readings.len()
    }
}

pub struct RxDataChannels {
    pub mem_usage: kanal::AsyncReceiver<Timestamped<MemoryUsageInfo>>,
    pub cpu_temp: kanal::AsyncReceiver<Timestamped<ThermodynamicTemperature>>,
    pub fs_usage: kanal::AsyncReceiver<Timestamped<FilesystemUsageInfo>>,
    pub bno_reading: kanal::AsyncReceiver<Timestamped<bno_055::Bno055Reading>>,
    pub tel0157_reading: kanal::AsyncReceiver<Timestamped<tel0157::Tel0157Reading>>,
    pub bmp280_reading: kanal::AsyncReceiver<Timestamped<Labeled<bmp280::Bmp280Reading>>>,
    pub as7341_reading: kanal::AsyncReceiver<NewAs7341Reading>,
}
