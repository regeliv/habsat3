//! BNO-055 Linux userspace driver.
//!
//! The document excerpts mentioned in comments  refer to the "BNO055: data sheet" revsion 1.8 (October 2021)
//! available at https://cdn-learn.adafruit.com/assets/assets/000/125/776/original/bst-bno055-ds000.pdf?1698865246

use arbitrary_int::u4;
use bitbybit::{bitenum, bitfield};
use core::mem::size_of;
use i2cdev::core::I2CDevice;
use std::time::Duration;
use zerocopy::IntoBytes;

pub const BNO_055_I2C_ADDR: u8 = 0x28;

/// BNO-055 sensor
pub struct Bno055<Dev: I2CDevice> {
    device: Dev,
}

/// Register addresses on page 0.
///
/// Section 4.3
#[expect(dead_code)]
mod addresses {
    /// First sensor data byte
    pub const ACC_DATA_X_LSB: u8 = 0x08; // 4.3.9
    pub const SENSOR_DATA_START: u8 = ACC_DATA_X_LSB;

    /// First byte past main sensor data
    pub const TEMPERATURE: u8 = 0x34; // 4.3.53
    pub const SENSOR_DATA_END: u8 = TEMPERATURE;

    /// Calibration statuses of system, gysroscope, acclerometer, magnetometer
    pub const CALIBRATION_STATUS: u8 = 0x35; // 4.3.54

    /// Selected operating mode (e.g config, IMU)
    pub const OPERATING_MODE: u8 = 0x3d; // 4.3.61

    /// First byte of sensor calibration data
    pub const ACC_OFFSET_X_LSB: u8 = 0x55; // 4.3.85
    pub const SENSOR_CONFIG_START: u8 = ACC_OFFSET_X_LSB;

    /// Last byte of sensor calibration data
    pub const MAG_RADIUS_MSB: u8 = 0x6A;
    pub const SENSOR_CONFIG_END: u8 = MAG_RADIUS_MSB + 1;

    pub const SENSOR_DATA_LENGTH: u8 = SENSOR_DATA_END - SENSOR_DATA_START;
    pub const SENSOR_CONFIG_LENGTH: u8 = SENSOR_CONFIG_END - SENSOR_CONFIG_START;
}

impl<Dev: I2CDevice> Bno055<Dev> {
    pub fn new(device: Dev) -> Result<Self, Dev::Error> {
        let mut bno = Self { device };
        bno.set_operating_mode(OperatingMode::CONFIGMODE)?;

        Ok(bno)
    }

    pub fn get_operating_mode(&mut self) -> Result<OperatingMode, Dev::Error> {
        let mut buf = 0u8;

        self.device.write(&[addresses::OPERATING_MODE])?;
        self.device.read(buf.as_mut_bytes())?;

        let op_mode_reg = OperatingModeReg::new_with_raw_value(buf);

        Ok(op_mode_reg
            .operating_mode()
            .expect("Device should not give out reserved values for operating mode."))
    }

    pub fn set_operating_mode(&mut self, target_mode: OperatingMode) -> Result<(), Dev::Error> {
        self.device.write(&[
            addresses::OPERATING_MODE,
            OperatingModeReg::default()
                .with_operating_mode(target_mode)
                .raw_value(),
        ])?;

        // Table 2-5 states that the maximum mode switching time is 19ms. We add some extra to
        // that.
        std::thread::sleep(Duration::from_millis(30));

        Ok(())
    }

    pub fn get_sensor_config(&mut self) -> Result<SensorConfig, Dev::Error> {
        let mut sensor_config = SensorConfig::default();

        self.device.write(&[addresses::SENSOR_CONFIG_START])?;
        self.device.read(sensor_config.as_mut_bytes())?;

        Ok(sensor_config)
    }

    pub fn set_sensor_config(&mut self, sensor_config: &SensorConfig) -> Result<(), Dev::Error> {
        let mut buf = [0u8; 1 + size_of::<SensorConfig>()];

        buf[0] = addresses::SENSOR_CONFIG_START;
        buf[1..].copy_from_slice(sensor_config.as_bytes());

        self.device.write(&buf)?;

        Ok(())
    }

    pub fn get_sensor_data(&mut self) -> Result<SensorData, Dev::Error> {
        // BNO-055's register map is little endian and our target is little endian. We exploit this
        // quickly and cheaply cast sensor readings into our struct. Supporting big endian targets
        // would require bytswapping each u16, but there's no reason to bother with that. Thus, we
        // assert away the possibility of compiling on big endian targets
        #[cfg(target_endian = "big")]
        const _: () = assert!(false);

        let mut sensor_config = SensorData::default();

        self.device.write(&[addresses::SENSOR_DATA_START])?;
        self.device.read(sensor_config.as_mut_bytes())?;

        Ok(sensor_config)
    }
}

#[repr(C)]
#[derive(
    Debug,
    Default,
    PartialEq,
    Eq,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::Immutable,
    serde::Serialize,
    serde::Deserialize,
    Clone,
)]
pub struct SensorData {
    acc_x: u16,
    acc_y: u16,
    acc_z: u16,

    mag_x: u16,
    mag_y: u16,
    mag_z: u16,

    gyr_x: u16,
    gyr_y: u16,
    gyr_z: u16,

    eul_heading: u16,
    eul_roll: u16,
    eul_pitch: u16,

    qua_w: u16,
    qua_x: u16,
    qua_y: u16,
    qua_z: u16,

    lia_x: u16,
    lia_y: u16,
    lia_z: u16,

    grv_x: u16,
    grv_y: u16,
    grv_z: u16,
}
const _: () = assert!(size_of::<SensorData>() == addresses::SENSOR_DATA_LENGTH as usize);

#[repr(C)]
#[derive(
    Debug,
    Default,
    PartialEq,
    Eq,
    zerocopy::IntoBytes,
    zerocopy::FromBytes,
    zerocopy::Immutable,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct SensorConfig {
    acc_offset_x_lsb: u8,
    acc_offset_x_msb: u8,
    acc_offset_y_lsb: u8,
    acc_offset_y_msb: u8,
    acc_offset_z_lsb: u8,
    acc_offset_z_msb: u8,

    mag_offset_x_lsb: u8,
    mag_offset_x_msb: u8,
    mag_offset_y_lsb: u8,
    mag_offset_y_msb: u8,
    mag_offset_z_lsb: u8,
    mag_offset_z_msb: u8,

    gyr_offset_x_lsb: u8,
    gyr_offset_x_msb: u8,
    gyr_offset_y_lsb: u8,
    gyr_offset_y_msb: u8,
    gyr_offset_z_lsb: u8,
    gyr_offset_z_msb: u8,

    acc_radius_lsb: u8,
    acc_radius_msb: u8,
    mag_radius_lsb: u8,
    mag_radius_msb: u8,
}

const _: () =
    assert!(core::mem::size_of::<SensorConfig>() == addresses::SENSOR_CONFIG_LENGTH as usize);

/// Operation Mode of the sensor, decides the scope of data to be gathered.
///
/// Table 3-5
#[bitenum(u4, exhaustive = false)]
#[allow(non_camel_case_types)]
#[derive(Debug, PartialEq, Eq)]
pub enum OperatingMode {
    CONFIGMODE = 0,

    // Non fusion modes
    ACCONLY = 1,
    MAGONLY = 2,
    GYRONLY = 3,
    ACCMAG = 4,
    ACCGYRO = 5,
    MAGGYRO = 6,
    AMG = 7,

    // Fusion modes
    IMU = 8,
    COMPASS = 9,
    M4G = 10,
    NDOF_FMC_OFF = 11,
    NDOF = 12,
}

#[bitfield(u8, debug, default = 0)]
struct OperatingModeReg {
    #[bits(0..=3, rw)]
    operating_mode: Option<OperatingMode>,

    #[bits(4..=7, r)]
    reserved: u4,
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2cdev::mock::MockI2CDevice;

    #[test]
    fn test_new() {
        let mut device = MockI2CDevice::new();
        let op_mode = OperatingModeReg::default().with_operating_mode(OperatingMode::ACCMAG);

        device
            .regmap
            .write_regs(addresses::OPERATING_MODE as usize, &[op_mode.raw_value()]);

        let mut bno = Bno055::new(device).expect("Initialization must succeed in this test");

        assert_eq!(bno.get_operating_mode().unwrap(), OperatingMode::CONFIGMODE);
    }

    #[test]
    fn test_sensor_config() {
        let mut device = MockI2CDevice::new();

        let mut expected_sensor_config = SensorConfig::default();
        expected_sensor_config.as_mut_bytes().fill(0xAA);

        device.regmap.write_regs(
            addresses::SENSOR_CONFIG_START as usize,
            expected_sensor_config.as_bytes(),
        );

        let mut bno = Bno055::new(device).expect("Initialization must succeed in this test");

        assert_eq!(bno.get_sensor_config().unwrap(), expected_sensor_config);

        expected_sensor_config.as_mut_bytes().fill(0xBB);
        bno.set_sensor_config(&expected_sensor_config).unwrap();

        assert_eq!(bno.get_sensor_config().unwrap(), expected_sensor_config);
    }

    #[test]
    fn test_sensor_data() {
        let mut device = MockI2CDevice::new();

        let mut expected_sensor_data = SensorData::default();
        expected_sensor_data.as_mut_bytes().fill(0xCC);

        device.regmap.write_regs(
            addresses::SENSOR_DATA_START as usize,
            expected_sensor_data.as_bytes(),
        );

        let mut bno = Bno055::new(device).expect("Initialization must succeed in this test");

        assert_eq!(bno.get_sensor_data().unwrap(), expected_sensor_data);
    }
}
