use arbitrary_int::{u3, u5, u7};
use bitbybit::bitfield;
use i2cdev::core::I2CDevice;
use uom::si::{
    angle::degree,
    f64::{Angle, Length, Velocity},
    length::meter,
    velocity::knot,
};
use zerocopy::{FromZeros, IntoBytes};

pub const TEL0157_I2C_ADDR: u8 = 0x20;

pub struct Tel0157<Dev: I2CDevice> {
    device: Dev,
}

impl<Dev: I2CDevice> Tel0157<Dev> {
    pub fn new(device: Dev) -> Result<Self, Dev::Error> {
        let mut tel0157 = Self { device };

        tel0157.power_on()?;

        tel0157.set_gnss_mode(
            GnssMode::builder()
                .with_gps(true)
                .with_beidou(true)
                .with_glonass(true)
                .build(),
        )?;

        Ok(tel0157)
    }

    fn power_on(&mut self) -> Result<(), Dev::Error> {
        let turn_on = SleepRegister::default().with_sleep(false);

        self.device
            .write(&[addresses::SLEEP_REGISTER, turn_on.raw_value()])
    }

    fn set_gnss_mode(&mut self, mode: GnssMode) -> Result<(), Dev::Error> {
        let register = GnssModeRegister::default().with_mode(mode);

        self.device
            .write(&[addresses::GNSS_MODE_REGISTER, register.raw_value()])
    }

    fn get_register_map(&mut self) -> Result<RegisterMap, Dev::Error> {
        let mut register_map = RegisterMap::new_zeroed();

        self.device.write(&[addresses::REGISTER_MAP_START])?;
        self.device.read(register_map.as_mut_bytes())?;

        Ok(register_map)
    }

    pub fn reading(&mut self) -> Result<Tel0157Reading, Dev::Error> {
        let register_map = self.get_register_map()?;

        Ok(Tel0157Reading {
            longitude: register_map.longitude(),
            latitude: register_map.latitude(),
            course_over_ground: register_map.course_over_ground(),
            speed_over_ground: register_map.speed_over_ground(),
            altitude: register_map.altitude(),
            satellites: register_map.satellites,
        })
    }

    #[cfg(test)]
    fn get_sleep_register(&mut self) -> SleepRegister {
        let mut buf = 0u8;

        self.device.write(&[addresses::SLEEP_REGISTER]).unwrap();
        self.device.read(buf.as_mut_bytes()).unwrap();

        SleepRegister::new_with_raw_value(buf)
    }

    #[cfg(test)]
    fn get_mode_reigster(&mut self) -> GnssModeRegister {
        let mut buf = 0u8;

        self.device.write(&[addresses::GNSS_MODE_REGISTER]).unwrap();
        self.device.read(buf.as_mut_bytes()).unwrap();

        GnssModeRegister::new_with_raw_value(buf)
    }
}

mod addresses {
    /// Address of the first register that is relevant to us.
    /// We fetch large part of the whole register map to avoid unnecessary i2c traffic
    pub const REGISTER_MAP_START: u8 = 0x07;

    /// Address one past the last register relevant to us
    pub const REGISTER_MAP_END: u8 = 0x1d;

    pub const REGISTER_MAP_LEN: u8 = REGISTER_MAP_END - REGISTER_MAP_START;

    pub const GNSS_MODE_REGISTER: u8 = 0x22;
    pub const SLEEP_REGISTER: u8 = 0x23;
}

#[derive(Clone, Copy, Debug)]
pub struct Tel0157Reading {
    pub latitude: Angle,
    pub longitude: Angle,
    pub course_over_ground: Angle,
    pub speed_over_ground: Velocity,
    pub altitude: Length,
    pub satellites: u8,
}

#[derive(
    Clone, Debug, PartialEq, Eq, zerocopy::IntoBytes, zerocopy::Immutable, zerocopy::FromBytes,
)]
struct RegisterMap {
    pub lat_degrees: u8,
    pub lat_minutes: u8,
    pub lat_decimal_minutes_msb: u8,
    pub lat_decimal_minutes_xsb: u8,
    pub lat_decimal_minutes_lsb: u8,

    // 'E' or 'W'
    pub lon_hemisphere: u8,
    pub lon_degrees: u8,
    pub lon_minutes: u8,
    pub lon_decimal_minutes_msb: u8,
    pub lon_decimal_minutes_xsb: u8,
    pub lon_decimal_minutes_lsb: u8,

    // 'N' or 'S'
    pub lat_hemisphere: u8,

    pub satellites: u8,

    pub alt_msb: u8,
    pub alt_lsb: u8,
    pub alt_xsb: u8,

    pub sog_msb: u8,
    pub sog_lsb: u8,
    pub sog_xsb: u8,

    pub cog_msb: u8,
    pub cog_lsb: u8,
    pub cog_xsb: u8,
}

const _: () = assert!(size_of::<RegisterMap>() == addresses::REGISTER_MAP_LEN as usize);

impl RegisterMap {
    fn latitude(&self) -> Angle {
        let decimal_minutes = u32::from_be_bytes([
            0x00,
            self.lat_decimal_minutes_msb,
            self.lat_decimal_minutes_xsb,
            self.lat_decimal_minutes_lsb,
        ]);

        // It's a whole number between between 0 and 99'999, so we scale it down
        let decimal_minutes = decimal_minutes as f64 / 1e5;

        let degrees = self.lat_degrees as f64;
        let minutes = self.lat_minutes as f64;

        let total_degrees = degrees + (minutes + decimal_minutes) / 60.0;

        let hemisphere = if self.lat_hemisphere == b'N' {
            1.0
        } else {
            -1.0
        };

        Angle::new::<degree>(total_degrees * hemisphere)
    }

    fn longitude(&self) -> Angle {
        let decimal_minutes = u32::from_be_bytes([
            0x00,
            self.lon_decimal_minutes_msb,
            self.lon_decimal_minutes_xsb,
            self.lon_decimal_minutes_lsb,
        ]);

        // It's a whole number between between 0 and 99'999, so we scale it down
        let decimal_minutes = decimal_minutes as f64 / 1e5;

        let degrees = self.lon_degrees as f64;
        let minutes = self.lon_minutes as f64;

        let total_degrees = degrees + (minutes + decimal_minutes) / 60.0;

        let hemisphere = if self.lon_hemisphere == b'E' {
            1.0
        } else {
            -1.0
        };

        Angle::new::<degree>(total_degrees * hemisphere)
    }

    fn altitude(&self) -> Length {
        let alt = self.alt_msb as f64 * 256.0 + self.alt_xsb as f64 + self.alt_lsb as f64 / 100.0;

        Length::new::<meter>(alt)
    }

    fn course_over_ground(&self) -> Angle {
        let cog = self.cog_msb as f64 * 256.0 + self.cog_xsb as f64 + self.cog_lsb as f64 / 100.0;

        Angle::new::<degree>(cog)
    }

    fn speed_over_ground(&self) -> Velocity {
        let sog = self.sog_msb as f64 * 256.0 + self.sog_xsb as f64 + self.sog_lsb as f64 / 100.0;

        Velocity::new::<knot>(sog)
    }
}

#[bitfield(u3, debug, default = 0)]
struct GnssMode {
    #[bit(0, rw)]
    gps: bool,
    #[bit(1, rw)]
    beidou: bool,
    #[bit(2, rw)]
    glonass: bool,
}

#[bitfield(u8, debug, default = 0)]
struct GnssModeRegister {
    #[bits(0..=2, rw)]
    mode: GnssMode,

    #[bits(3..=7, r)]
    reserved: u5,
}

#[bitfield(u8, debug, default = 0)]
struct SleepRegister {
    #[bit(0, rw)]
    sleep: bool,
    #[bits(1..=7, r)]
    reserved: u7,
}

#[cfg(test)]
mod tests {
    use i2cdev::mock::MockI2CDevice;
    use uom::si::angle::degree;
    use zerocopy::{FromZeros, IntoBytes};

    use crate::{GnssMode, RegisterMap, Tel0157, addresses};

    #[test]
    fn test_latitude_conversion_svaalbard() {
        let mut reg_map = RegisterMap::new_zeroed();

        // 78°14′09″N
        reg_map.lat_hemisphere = b'N';
        reg_map.lat_degrees = 78;
        reg_map.lat_minutes = 14;
        // (9 / 60) * 10e5 = 0x3a98
        reg_map.lat_decimal_minutes_msb = 0x00;
        reg_map.lat_decimal_minutes_xsb = 0x3a;
        reg_map.lat_decimal_minutes_lsb = 0x98;

        assert_eq!(reg_map.latitude().get::<degree>(), 78.23583333333333);
    }

    #[test]
    fn test_latitude_conversion_salar_de_uyuni() {
        let mut reg_map = RegisterMap::new_zeroed();

        // 20°08′01.59″S
        reg_map.lat_hemisphere = b'S';
        reg_map.lat_degrees = 20;
        reg_map.lat_minutes = 8;
        // (1.59 / 60) * 10e5 = 0x0a5a
        reg_map.lat_decimal_minutes_msb = 0x00;
        reg_map.lat_decimal_minutes_xsb = 0x0a;
        reg_map.lat_decimal_minutes_lsb = 0x5a;

        assert_eq!(reg_map.latitude().get::<degree>(), -20.133775);
    }

    #[test]
    fn test_longitude_conversion_lake_natron() {
        let mut reg_map = RegisterMap::new_zeroed();

        // 36°00′59.99″E
        reg_map.lon_hemisphere = b'E';
        reg_map.lon_degrees = 36;
        reg_map.lon_minutes = 00;
        // (59.99 / 60) * 10e5 = 0x01868f
        reg_map.lon_decimal_minutes_msb = 0x01;
        reg_map.lon_decimal_minutes_xsb = 0x86;
        reg_map.lon_decimal_minutes_lsb = 0x8f;

        assert_eq!(reg_map.longitude().get::<degree>(), 36.01666383333333);
    }

    #[test]
    fn test_longitude_conversion_salar_de_uyuni() {
        let mut reg_map = RegisterMap::new_zeroed();

        // 67°29′20.88″W
        reg_map.lon_hemisphere = b'W';
        reg_map.lon_degrees = 67;
        reg_map.lon_minutes = 29;
        // (20.88 / 60) * 10e5 = 0x87f0
        reg_map.lon_decimal_minutes_msb = 0x00;
        reg_map.lon_decimal_minutes_xsb = 0x87;
        reg_map.lon_decimal_minutes_lsb = 0xf0;

        assert_eq!(reg_map.longitude().get::<degree>(), -67.48913333333333);
    }

    #[test]
    fn test_new() {
        let mut device = MockI2CDevice::new();

        let mut register_map = RegisterMap::new_zeroed();
        register_map.as_mut_bytes().fill(0xab);

        device.regmap.write_regs(0, register_map.as_bytes());

        let mut tel0157 = Tel0157::new(device).unwrap();

        assert!(!tel0157.get_sleep_register().sleep());
        assert_eq!(
            tel0157.get_mode_reigster().mode().raw_value(),
            GnssMode::builder()
                .with_gps(true)
                .with_beidou(true)
                .with_glonass(true)
                .build()
                .raw_value()
        );
    }

    #[test]
    pub fn test_readings() {
        let mut device = MockI2CDevice::new();

        let mut expected = RegisterMap::new_zeroed();
        expected.as_mut_bytes().fill(0xab);

        device
            .regmap
            .write_regs(addresses::REGISTER_MAP_START as usize, expected.as_bytes());

        let mut tel0157 = Tel0157::new(device).unwrap();
        let interesting_register_map = tel0157.get_register_map().unwrap();

        assert_eq!(interesting_register_map, expected);
    }
}
