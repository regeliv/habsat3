use diesel::prelude::*;
use system_sensors::{FilesystemUsageInfo, MemoryUsageInfo};
use uom::si::{
    angle::degree, f64::ThermodynamicTemperature, information::mebibyte, length::meter,
    pressure::pascal, thermodynamic_temperature::degree_celsius, velocity::meter_per_second,
};

use crate::types::{Labeled, Timestamped};

#[derive(Insertable, Clone, Debug)]
#[diesel(table_name = crate::db::schema::filesystem_usage)]
pub struct NewFsUsage {
    pub timestamp: f64,
    pub free_mib: f64,
    pub total_mib: f64,
}

#[derive(Insertable, Clone, Debug)]
#[diesel(table_name = crate::db::schema::memory_usage)]
pub struct NewMemoryUsage {
    pub timestamp: f64,
    pub available_mib: f64,
    pub total_mib: f64,
}

#[derive(Insertable, Clone, Debug)]
#[diesel(table_name = crate::db::schema::cpu_temperature)]
pub struct NewCpuTemperature {
    pub timestamp: f64,
    pub degrees_celsius: f64,
}

#[derive(Insertable, Clone, Debug)]
#[diesel(table_name = crate::db::schema::bno055_readings)]
pub struct NewBnoReading {
    pub timestamp: f64,
    pub acc_x: i32,
    pub acc_y: i32,
    pub acc_z: i32,
    pub mag_x: i32,
    pub mag_y: i32,
    pub mag_z: i32,
    pub gyr_x: i32,
    pub gyr_y: i32,
    pub gyr_z: i32,
    pub eul_heading: i32,
    pub eul_roll: i32,
    pub eul_pitch: i32,
    pub qua_w: i32,
    pub qua_x: i32,
    pub qua_y: i32,
    pub qua_z: i32,
    pub lia_x: i32,
    pub lia_y: i32,
    pub lia_z: i32,
    pub grv_x: i32,
    pub grv_y: i32,
    pub grv_z: i32,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = crate::db::schema::tel0157_readings)]
pub struct NewTel0157Reading {
    pub timestamp: f64,
    pub latitude_degrees: f64,
    pub longitude_degrees: f64,
    pub course_over_ground_degrees: f64,
    pub speed_over_ground_meters_per_second: f64,
    pub altitude_meters: f64,
    pub satellites: i32,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = crate::db::schema::bmp280_readings)]
pub struct NewBmp280Reading {
    pub timestamp: f64,
    pub bmp_id: i32,
    pub temperature_degrees_celsius: f64,
    pub pressure_pascals: f64,
}

pub trait NewFromTimestamped {
    type Source: Clone + std::fmt::Debug;
    fn new_from_timestamped(data: &Timestamped<Self::Source>) -> Self;
}

impl NewFromTimestamped for NewCpuTemperature {
    type Source = ThermodynamicTemperature;

    fn new_from_timestamped(data: &Timestamped<Self::Source>) -> Self {
        Self {
            timestamp: data.timestamp.as_secs(),
            degrees_celsius: data.data.get::<degree_celsius>(),
        }
    }
}

impl NewFromTimestamped for NewMemoryUsage {
    type Source = MemoryUsageInfo;

    fn new_from_timestamped(data: &Timestamped<Self::Source>) -> Self {
        Self {
            timestamp: data.timestamp.as_secs(),
            available_mib: data.data.available.get::<mebibyte>(),
            total_mib: data.data.total.get::<mebibyte>(),
        }
    }
}

impl NewFromTimestamped for NewFsUsage {
    type Source = FilesystemUsageInfo;

    fn new_from_timestamped(data: &Timestamped<Self::Source>) -> Self {
        Self {
            timestamp: data.timestamp.as_secs(),
            free_mib: data.data.free.get::<mebibyte>(),
            total_mib: data.data.total.get::<mebibyte>(),
        }
    }
}

impl NewFromTimestamped for NewBnoReading {
    type Source = bno_055::Bno055Reading;

    fn new_from_timestamped(data: &Timestamped<Self::Source>) -> Self {
        Self {
            timestamp: data.timestamp.as_secs(),
            acc_x: data.data.acc_x as i32,
            acc_y: data.data.acc_y as i32,
            acc_z: data.data.acc_z as i32,
            mag_x: data.data.mag_x as i32,
            mag_y: data.data.mag_y as i32,
            mag_z: data.data.mag_z as i32,
            gyr_x: data.data.gyr_x as i32,
            gyr_y: data.data.gyr_y as i32,
            gyr_z: data.data.gyr_z as i32,
            eul_heading: data.data.eul_heading as i32,
            eul_roll: data.data.eul_roll as i32,
            eul_pitch: data.data.eul_pitch as i32,
            qua_w: data.data.qua_w as i32,
            qua_x: data.data.qua_x as i32,
            qua_y: data.data.qua_y as i32,
            qua_z: data.data.qua_z as i32,
            lia_x: data.data.lia_x as i32,
            lia_y: data.data.lia_y as i32,
            lia_z: data.data.lia_z as i32,
            grv_x: data.data.grv_x as i32,
            grv_y: data.data.grv_y as i32,
            grv_z: data.data.grv_z as i32,
        }
    }
}

impl NewFromTimestamped for NewTel0157Reading {
    type Source = tel0157::Tel0157Reading;

    fn new_from_timestamped(data: &Timestamped<Self::Source>) -> Self {
        Self {
            timestamp: data.timestamp.as_secs(),
            latitude_degrees: data.data.latitude.get::<degree>(),
            longitude_degrees: data.data.longitude.get::<degree>(),
            course_over_ground_degrees: data.data.course_over_ground.get::<degree>(),
            speed_over_ground_meters_per_second: data
                .data
                .speed_over_ground
                .get::<meter_per_second>(),
            altitude_meters: data.data.altitude.get::<meter>(),
            satellites: data.data.satellites as i32,
        }
    }
}

impl NewFromTimestamped for NewBmp280Reading {
    type Source = Labeled<bmp280::Bmp280Reading>;

    fn new_from_timestamped(data: &Timestamped<Self::Source>) -> Self {
        Self {
            timestamp: data.timestamp.as_secs(),
            bmp_id: data.data.label as i32,
            // With some many `data`s we are likely too generic...
            temperature_degrees_celsius: data.data.data.temperature.get::<degree_celsius>(),
            pressure_pascals: data.data.data.pressure.get::<pascal>(),
        }
    }
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = crate::db::schema::as7341_readings)]
pub struct NewAs7341Reading {
    pub timestamp: f64,

    pub timeout: bool,

    pub nm415: i32,
    pub nm445: i32,
    pub nm480: i32,
    pub nm515: i32,
    pub nm555: i32,
    pub nm590: i32,
    pub nm630: i32,
    pub nm680: i32,
    pub nir: i32,
}
