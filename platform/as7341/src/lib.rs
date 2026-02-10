//! Docs:
//! AS7341: https://cdn.sparkfun.com/assets/0/8/e/2/3/AS7341_DS000504_3-00.pdf
//! SMUX Configuration PDF found in https://ams-osram.com/o/download-server/document-download/download/29941159

use std::time::Duration;

use arbitrary_int::{u3, u7};
use bitbybit::bitfield;
use i2cdev::{
    core::I2CDevice,
    linux::{LinuxI2CDevice, LinuxI2CError},
};
use thiserror::Error;
use zerocopy::{FromZeros, IntoBytes};

use crate::integration_time::TimingSettings;

pub const AS7341_ADDRESS: u8 = 0x39;

pub struct As7341<Dev: I2CDevice> {
    device: Dev,
}

mod addresses {
    pub const SMUX_START: u8 = 0x00;

    pub const ENABLE: u8 = 0x80;

    pub const ATIME: u8 = 0x81;
    pub const WTIME: u8 = 0x83;

    pub const CH0_DATA_L: u8 = 0x95;

    pub const CFG1: u8 = 0xaa;
    pub const CFG6: u8 = 0xaf;

    pub const ASTEP_L: u8 = 0xca;

    pub const AZ_CFG: u8 = 0xd6;
}

impl<Dev> As7341<Dev>
where
    Dev: I2CDevice,
    DeviceError<Dev>: std::convert::From<<Dev as i2cdev::core::I2CDevice>::Error>,
{
    pub fn new(device: Dev) -> Result<Self, Dev::Error> {
        let mut as7341 = Self { device };

        let disable_all = EnableRegister::ZERO;
        as7341.set_enable(disable_all)?;

        let power_on = disable_all.with_power_on(true).with_wait_enable(true);
        as7341.set_enable(power_on)?;

        as7341.get_enable()?;

        // The temperature will change greatly over time, so we autozero every cycle
        as7341.set_autozero_frequency(1)?;

        Ok(as7341)
    }

    pub fn set_gain(&mut self, gain: Gain) -> Result<(), Dev::Error> {
        self.device.write(&[addresses::CFG1, gain as u8])
    }

    fn set_smux_command(&mut self, cmd: SmuxCommand) -> Result<(), Dev::Error> {
        self.device.write(&[addresses::CFG6, cmd as u8])
    }

    fn set_enable(&mut self, register_value: EnableRegister) -> Result<(), Dev::Error> {
        self.device
            .write(&[addresses::ENABLE, register_value.raw_value()])
    }

    fn get_enable(&mut self) -> Result<EnableRegister, Dev::Error> {
        let mut raw_register = 0u8;

        self.device.write(&[addresses::ENABLE])?;
        self.device.read(std::slice::from_mut(&mut raw_register))?;

        Ok(EnableRegister::new_with_raw_value(raw_register))
    }

    pub fn set_spectral_measurement(&mut self, value: bool) -> Result<(), Dev::Error> {
        let enable_register = self.get_enable()?;

        if enable_register.spectral_measurement_enable() != value {
            self.set_enable(enable_register.with_spectral_measurement_enable(value))
        } else {
            Ok(())
        }
    }

    /// Connects pixels to channels according to the configuration.
    ///
    /// Leaves the Spectral Measurement Enable bit disabled
    pub async fn set_pixel_connections(
        &mut self,
        pixel_connections: &PixelConnections,
    ) -> Result<(), DeviceError<Dev>> {
        self.set_spectral_measurement(false)?;
        self.set_smux_command(SmuxCommand::Write)?;

        let mut bytes = [0u8; 21];

        bytes[0] = addresses::SMUX_START;
        bytes[1..].copy_from_slice(pixel_connections.as_bytes());

        self.device.write(&bytes)?;

        let enable_register = self.get_enable()?;
        self.set_enable(enable_register.with_smux_enable(true))?;

        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(1)).await;
            if !self.get_enable()?.smux_enable() {
                return Ok(());
            }
        }
        Err(DeviceError::SmuxTimeout)
    }

    pub fn set_timing(&mut self, timing_settings: TimingSettings) -> Result<(), Dev::Error> {
        let TimingSettings {
            integration_time,
            wait_time,
        } = timing_settings;
        let atime = integration_time.atime();
        let wtime = wait_time.into_raw();

        let astep = integration_time.astep().to_le_bytes();

        self.device.write(&[addresses::ATIME, atime])?;
        self.device.write(&[addresses::WTIME, wtime])?;

        self.device.write(&[addresses::ASTEP_L, astep[0], astep[1]])
    }

    fn set_autozero_frequency(&mut self, freq: u8) -> Result<(), Dev::Error> {
        self.device.write(&[addresses::AZ_CFG, freq])
    }

    pub async fn read_channels(
        &mut self,
        connections: &PixelConnections,
        polling_config: PollingConfig,
    ) -> Result<ChannelData, DeviceError<Dev>> {
        self.set_pixel_connections(connections).await?;

        self.set_spectral_measurement(true)?;

        let mut channel_data = ChannelData::new_zeroed();
        for _ in 0..polling_config.number_of_intervals {
            tokio::time::sleep(polling_config.polling_interval).await;

            self.device.write(&[addresses::CH0_DATA_L])?;
            self.device.read(channel_data.as_mut_bytes())?;

            if Status2::new_with_raw_value(channel_data.status2).spectral_valid() {
                return Ok(channel_data);
            }
        }

        Err(DeviceError::ChannelTimeout(channel_data))
    }
}

#[derive(Debug, Error)]
pub enum DeviceError<Dev: I2CDevice> {
    #[error("AS7341 I2C bus error: {0:?}")]
    I2C(Dev::Error),

    #[error("SMUX write timeout")]
    SmuxTimeout,

    #[error("Channel read timeout")]
    ChannelTimeout(ChannelData),
}

impl From<LinuxI2CError> for DeviceError<LinuxI2CDevice> {
    fn from(value: LinuxI2CError) -> Self {
        Self::I2C(value)
    }
}

pub const F1_PIXELS: &[Pixel] = &[Pixel::new(2), Pixel::new(32)];
pub const F2_PIXELS: &[Pixel] = &[Pixel::new(10), Pixel::new(25)];
pub const F3_PIXELS: &[Pixel] = &[Pixel::new(1), Pixel::new(31)];
pub const F4_PIXELS: &[Pixel] = &[Pixel::new(11), Pixel::new(26)];
pub const F5_PIXELS: &[Pixel] = &[Pixel::new(13), Pixel::new(19)];
pub const F6_PIXELS: &[Pixel] = &[Pixel::new(8), Pixel::new(29)];
pub const F7_PIXELS: &[Pixel] = &[Pixel::new(14), Pixel::new(20)];
pub const F8_PIXELS: &[Pixel] = &[Pixel::new(7), Pixel::new(28)];
pub const CLEAR_PIXELS: &[Pixel] = &[Pixel::new(17), Pixel::new(35)];
pub const NIR_PIXELS: &[Pixel] = &[Pixel::new(38)];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Nibble {
    Lower,
    Upper,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adc {
    Disabled = 0,
    Adc0 = 1,
    Adc1 = 2,
    Adc2 = 3,
    Adc3 = 4,
    Adc4 = 5,
    Adc5 = 6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pixel {
    id: u8,
}

impl Pixel {
    const fn new(id: u8) -> Self {
        Self { id }
    }

    const fn addr(&self) -> (usize, Nibble) {
        let i2c_address = (self.id / 2) as usize;
        let nibble = if self.id.is_multiple_of(2) {
            Nibble::Lower
        } else {
            Nibble::Upper
        };

        (i2c_address, nibble)
    }
}

#[derive(Clone, Copy, Debug, zerocopy::IntoBytes, zerocopy::FromBytes, zerocopy::Immutable)]
pub struct PixelConnections {
    raw_bytes: [u8; 20],
}

impl PixelConnections {
    pub const fn empty() -> Self {
        Self {
            raw_bytes: [0u8; 20],
        }
    }

    pub const fn connect_pixel(&mut self, pixel: Pixel, adc: Adc) {
        let (i2c_address, nibble) = pixel.addr();
        let current_byte_value = self.raw_bytes[i2c_address];

        let new_byte_value = match nibble {
            Nibble::Lower => (current_byte_value & 0xf0) | adc as u8,
            Nibble::Upper => (current_byte_value & 0x0f) | ((adc as u8) << 4),
        };

        self.raw_bytes[i2c_address] = new_byte_value;
    }

    pub const fn connect_pixels(mut self, pixels: &[Pixel], adc: Adc) -> Self {
        // Iterators are not available in `const fn`s, so we use a `while` loop
        let mut i = 0;
        while i < pixels.len() {
            self.connect_pixel(pixels[i], adc);
            i += 1;
        }

        self
    }

    pub const fn f1_f5_nir() -> Self {
        Self::empty()
            .connect_pixels(F1_PIXELS, Adc::Adc0)
            .connect_pixels(F2_PIXELS, Adc::Adc1)
            .connect_pixels(F3_PIXELS, Adc::Adc2)
            .connect_pixels(F4_PIXELS, Adc::Adc3)
            .connect_pixels(F5_PIXELS, Adc::Adc4)
            .connect_pixels(NIR_PIXELS, Adc::Adc5)
    }

    pub const fn f6_f8() -> Self {
        Self::empty()
            .connect_pixels(F6_PIXELS, Adc::Adc0)
            .connect_pixels(F7_PIXELS, Adc::Adc1)
            .connect_pixels(F8_PIXELS, Adc::Adc2)
    }
}

pub mod integration_time {
    const WAIT_TIME_STEP_SIZE: Duration = Duration::from_micros(2780);
    const MAX_WAIT_TIME: Duration =
        Duration::from_micros(WAIT_TIME_STEP_SIZE.as_micros() as u64 * 256);

    const INTEGRATION_TIME_STEP_SIZE: Duration = Duration::from_nanos(2780);

    /// The maximum integration duration with ATIME pegged to 0
    const MAX_INTEGRATION_TIME: Duration =
        Duration::from_nanos(INTEGRATION_TIME_STEP_SIZE.as_nanos() as u64 * 65535);

    pub struct TimingSettings {
        pub wait_time: WaitTime,
        pub integration_time: IntegrationTime,
    }

    impl TimingSettings {
        pub const fn new(integration_duration: Duration, wait_time_duration: Duration) -> Self {
            Self {
                wait_time: WaitTime::from_duration(wait_time_duration),
                integration_time: IntegrationTime::from_duration(integration_duration),
            }
        }
    }

    use std::time::Duration;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct WaitTime {
        raw_wait_time: u8,
    }

    impl WaitTime {
        pub const fn from_duration(duration: Duration) -> Self {
            assert!(
                WAIT_TIME_STEP_SIZE.as_nanos() <= duration.as_nanos(),
                "Minimum wait time is 2.78ms"
            );
            assert!(
                duration.as_nanos() <= MAX_WAIT_TIME.as_nanos(),
                "Maximum wait time is 711ms"
            );

            Self {
                raw_wait_time: (duration.as_nanos() / WAIT_TIME_STEP_SIZE.as_nanos()) as u8,
            }
        }

        pub const fn from_raw(value: u8) -> Self {
            Self {
                raw_wait_time: value,
            }
        }

        pub const fn into_raw(self) -> u8 {
            self.raw_wait_time
        }

        pub const fn into_duration(self) -> Duration {
            WAIT_TIME_STEP_SIZE
                .checked_mul(self.raw_wait_time as u32)
                .unwrap()
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct IntegrationTime {
        atime: u8,
        astep: u16,
    }

    impl IntegrationTime {
        pub const fn new(atime: u8, astep: u16) -> IntegrationTime {
            // From docs:
            // 'It is not allowed that both settings – ATIME and ASTEP – are set to "0"'
            assert!(
                atime != 0 || astep != 0,
                "At least one of [ATIME, ASTEP] must be non-zero"
            );

            // From docs:
            // '65535 reserved, do not use'
            assert!(astep != u16::MAX, "65535 is invalid for ASTEP");

            Self { atime, astep }
        }

        pub const fn from_duration(duration: Duration) -> Self {
            assert!(
                duration.as_nanos() <= MAX_INTEGRATION_TIME.as_nanos(),
                "Maximum integration time is 182ms"
            );
            assert!(
                // Because the ATIME register cannot be set to zero when ASTEP is zero, the real
                // minimum is not INTEGRATION_TIME_STEP_SIZE
                INTEGRATION_TIME_STEP_SIZE.as_nanos() * 2 <= duration.as_nanos(),
                "Minimum integration time is 5.56us"
            );

            let atime = 0;

            // The product: (ATIME + 1) (u9)  * (ASTEP + 1) (u17) is at most u26, so we use u32 here
            let astep_plus_1_times_atime_plus_1 =
                (duration.as_nanos() / INTEGRATION_TIME_STEP_SIZE.as_nanos()) as u32;

            let mut astep = (astep_plus_1_times_atime_plus_1 / (atime as u32 + 1)) as u16;
            astep = astep.saturating_sub(1);

            IntegrationTime::new(atime, astep)
        }

        pub const fn into_duration(self) -> Duration {
            Duration::from_nanos(2780)
                .checked_mul(self.astep as u32 + 1)
                .unwrap()
                .checked_mul(self.atime as u32 + 1)
                .unwrap()
        }

        pub const fn astep(&self) -> u16 {
            self.astep
        }

        pub const fn atime(&self) -> u8 {
            self.atime
        }
    }
}

#[derive(Debug, Clone, Copy, zerocopy::FromBytes, zerocopy::IntoBytes)]
pub struct ChannelData {
    pub ch0_data: u16,
    pub ch1_data: u16,
    pub ch2_data: u16,
    pub ch3_data: u16,
    pub ch4_data: u16,
    pub ch5_data: u16,

    pub reserved: [u8; 2],

    pub status2: u8,

    // We don't really need it, but it's necessary to include it so that we don't have padding
    // that would prevent us from implementing `zerocopy` traits
    pub status3: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct PollingConfig {
    pub polling_interval: Duration,
    pub number_of_intervals: usize,
}

#[bitfield(u8, debug, default = 0)]
struct Status2 {
    #[bits([0..=5, 7], r)]
    reserved: u7,

    #[bit(6, r)]
    spectral_valid: bool,
}

#[bitfield(u8, debug, default = 0)]
struct EnableRegister {
    #[bit(0, rw)]
    power_on: bool,

    #[bit(1, rw)]
    spectral_measurement_enable: bool,

    #[bit(3, rw)]
    wait_enable: bool,

    #[bit(4, rw)]
    smux_enable: bool,

    #[bit(6, rw)]
    flicker_detection_enable: bool,

    #[bits([2,5,7], r)]
    reserved: u3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum SmuxCommand {
    _RomInit = 0 << 3,
    _Read = 1 << 3,
    Write = 2 << 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Gain {
    X0_5 = 0,
    X1 = 1,
    X2 = 2,
    X4 = 3,
    X8 = 4,
    X16 = 5,
    X32 = 6,
    X64 = 7,
    X128 = 8,
    X256 = 9,
    X512 = 10,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use zerocopy::IntoBytes;

    use crate::integration_time::{IntegrationTime, WaitTime};

    use super::*;
    #[test]
    pub fn test_pixel_address_mapping() {
        let pixel1 = Pixel::new(1);
        assert_eq!(pixel1.addr(), (0x00, Nibble::Upper));

        let pixel2 = Pixel::new(2);
        assert_eq!(pixel2.addr(), (0x01, Nibble::Lower));

        let pixel7 = Pixel::new(7);
        assert_eq!(pixel7.addr(), (0x03, Nibble::Upper));

        let pixel8 = Pixel::new(8);
        assert_eq!(pixel8.addr(), (0x04, Nibble::Lower));

        let pixel10 = Pixel::new(10);
        assert_eq!(pixel10.addr(), (0x05, Nibble::Lower));

        let pixel11 = Pixel::new(11);
        assert_eq!(pixel11.addr(), (0x05, Nibble::Upper));

        let pixel13 = Pixel::new(13);
        assert_eq!(pixel13.addr(), (0x06, Nibble::Upper));

        let pixel14 = Pixel::new(14);
        assert_eq!(pixel14.addr(), (0x07, Nibble::Lower));

        let pixel17 = Pixel::new(17);
        assert_eq!(pixel17.addr(), (0x08, Nibble::Upper));

        let pixel19 = Pixel::new(19);
        assert_eq!(pixel19.addr(), (0x09, Nibble::Upper));

        let pixel20 = Pixel::new(20);
        assert_eq!(pixel20.addr(), (0x0A, Nibble::Lower));

        let pixel25 = Pixel::new(25);
        assert_eq!(pixel25.addr(), (0x0C, Nibble::Upper));

        let pixel26 = Pixel::new(26);
        assert_eq!(pixel26.addr(), (0x0D, Nibble::Lower));

        let pixel28 = Pixel::new(28);
        assert_eq!(pixel28.addr(), (0x0E, Nibble::Lower));

        let pixel29 = Pixel::new(29);
        assert_eq!(pixel29.addr(), (0x0E, Nibble::Upper));

        let pixel31 = Pixel::new(31);
        assert_eq!(pixel31.addr(), (0x0F, Nibble::Upper));

        let pixel32 = Pixel::new(32);
        assert_eq!(pixel32.addr(), (0x10, Nibble::Lower));

        let pixel35 = Pixel::new(35);
        assert_eq!(pixel35.addr(), (0x11, Nibble::Upper));

        let pixel38 = Pixel::new(38);
        assert_eq!(pixel38.addr(), (0x13, Nibble::Lower));
    }

    #[test]
    fn test_as_bytes_f1_f4_clear_nir() {
        let raw_bytes: [u8; 20] = [
            0x30, 0x01, 0x00, 0x00, 0x00, 0x42, 0x00, 0x00, 0x50, 0x00, 0x00, 0x00, 0x20, 0x04,
            0x00, 0x30, 0x01, 0x50, 0x00, 0x06,
        ];

        assert_eq!(PixelConnections::f1_f4_clear_nir().as_bytes(), &raw_bytes);
    }

    #[test]
    fn test_as_bytes_f5_f8() {
        let raw_bytes: [u8; 20] = [
            0x00, 0x00, 0x00, 0x40, 0x02, 0x00, 0x10, 0x03, 0x00, 0x10, 0x03, 0x00, 0x00, 0x00,
            0x24, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        assert_eq!(PixelConnections::f5_f8().as_bytes(), &raw_bytes);
    }

    #[test]
    #[should_panic(expected = "At least one of [ATIME, ASTEP] must be non-zero")]
    fn test_integration_time_new_0_0() {
        IntegrationTime::new(0, 0);
    }

    #[test]
    #[should_panic(expected = "65535 is invalid for ASTEP")]
    fn test_integration_time_new_invalid() {
        IntegrationTime::new(0, 65535);
    }

    #[test]
    fn test_integration_time_ok() {
        IntegrationTime::new(0, 65534);
        IntegrationTime::new(255, 65534);
        IntegrationTime::new(123, 456);
        IntegrationTime::new(123, 0);
    }

    #[test]
    fn test_inegration_time_from_duration_ok() {
        let int_time = IntegrationTime::from_duration(Duration::from_micros(712));
        assert_eq!(int_time.into_duration(), Duration::from_nanos(711680));

        let int_time = IntegrationTime::from_duration(Duration::from_millis(3));
        assert_eq!(
            int_time.into_duration(),
            Duration::from_secs_f64(0.00299962)
        );
        assert_eq!(int_time.astep(), 1078);
        assert_eq!(int_time.atime(), 0);

        let int_time = IntegrationTime::from_duration(Duration::from_millis(50));
        assert_eq!(int_time.into_duration(), Duration::from_secs_f64(0.0499983));

        let int_time = IntegrationTime::from_duration(Duration::from_micros(182187));
        assert_eq!(
            int_time.into_duration(),
            Duration::from_secs_f64(0.18218452)
        );
        assert_eq!(int_time.astep(), 65533);
        assert_eq!(int_time.atime(), 0);
    }

    #[test]
    #[should_panic(expected = "Maximum integration time is 182ms")]
    fn test_inegration_time_from_duration_too_big() {
        IntegrationTime::from_duration(Duration::from_millis(183));
    }

    #[test]
    #[should_panic(expected = "Minimum integration time is 5.56us")]
    fn test_inegration_time_from_duration_too_small() {
        IntegrationTime::from_duration(Duration::from_micros(5));
    }

    #[test]
    #[should_panic(expected = "Maximum wait time is 711ms")]
    fn test_wait_time_too_big() {
        WaitTime::from_duration(Duration::from_millis(712));
    }

    #[test]
    #[should_panic(expected = "Minimum wait time is 2.78ms")]
    fn test_wait_time_too_small() {
        let wt = WaitTime::from_duration(Duration::from_millis(0));
        assert_eq!(wt, WaitTime::from_raw(0));
    }

    #[test]
    fn test_wait_time_ok() {
        let wt = WaitTime::from_duration(Duration::from_millis(711));
        assert_eq!(wt, WaitTime::from_raw(255));

        let wt = WaitTime::from_duration(Duration::from_millis(356));
        assert_eq!(wt, WaitTime::from_raw(128));

        let wt = WaitTime::from_duration(Duration::from_millis(3));
        assert_eq!(wt, WaitTime::from_raw(1));
    }

    #[test]
    fn test_wait_time_into_raw() {
        let wt = WaitTime::from_duration(Duration::from_micros(2780));
        assert_eq!(wt.into_duration(), Duration::from_micros(2780));

        let wt = WaitTime::from_duration(Duration::from_millis(356));
        assert_eq!(wt.into_duration(), Duration::from_micros(355840));
    }
}
