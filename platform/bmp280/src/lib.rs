use arbitrary_int::{u5, u7};
use bitbybit::{bitenum, bitfield};
use i2cdev::core::I2CDevice;
use std::time::Duration;
use uom::si::{
    f64::{Pressure, ThermodynamicTemperature},
    pressure::pascal,
    thermodynamic_temperature::degree_celsius,
};
use zerocopy::{FromZeros, IntoBytes};

#[derive(Debug, Clone)]
pub struct Bmp280<Dev: I2CDevice> {
    device: Dev,
    trimming_parameters: TrimmingParameters,
}

#[derive(Debug, Clone)]
pub struct Bmp280Reading {
    pub pressure: Pressure,
    pub temperature: ThermodynamicTemperature,
}

impl<Dev: I2CDevice> Bmp280<Dev> {
    pub fn new(mut device: Dev) -> Result<Self, Dev::Error> {
        let mut trimming_parameters = TrimmingParameters::new_zeroed();

        device.write(&[addresses::TRIMMING_PARAMETERS_START])?;
        device.read(trimming_parameters.as_mut_bytes())?;

        let mut bmp280 = Self {
            device,
            trimming_parameters,
        };

        bmp280.set_filter(Filter::X16)?;

        Ok(bmp280)
    }

    pub async fn reading(&mut self) -> Result<Bmp280Reading, Dev::Error> {
        self.force_measurement()?;

        // Really, we should be probing measurement status instead of sleeping,
        // but in order to keep things simple we do an async sleep.
        //
        // The easurement time for X16 pressure and X2 temperature
        // oversampling is at most 43.2ms (chapter 3.8.1 in Bosch docs).
        //
        // We sleep a bit longer than that to give the sensor a bit more time
        //
        // If we undersleep though, it's not end of the world:
        // we'll just get inconsistent or old results
        tokio::time::sleep(Duration::from_millis(50)).await;

        let pressure_and_temperature: PressureAndTemperature =
            self.get_pressure_and_temperature()?;

        let (t_fine, temperature) = pressure_and_temperature.temperature(&self.trimming_parameters);
        let pressure = pressure_and_temperature.pressure(&self.trimming_parameters, t_fine);

        Ok(Bmp280Reading {
            pressure,
            temperature,
        })
    }

    fn set_filter(&mut self, filter: Filter) -> Result<(), Dev::Error> {
        let config_reg = ConfigReg::default().with_filter(filter);

        self.device
            .write(&[addresses::CONFIG_ADDRESS, config_reg.raw_value()])
    }

    fn force_measurement(&mut self) -> Result<(), Dev::Error> {
        let ctrl_meas_reg = CtrlMeasReg::builder()
            .with_power_mode(PowerMode::Forced)
            .with_pressure_oversampling(Oversampling::X16)
            .with_temperature_oversampling(Oversampling::X2)
            .build();

        self.device
            .write(&[addresses::CTRL_MEAS_ADDRESS, ctrl_meas_reg.raw_value()])
    }

    fn get_pressure_and_temperature(&mut self) -> Result<PressureAndTemperature, Dev::Error> {
        let mut pressure_and_temperature = PressureAndTemperature::new_zeroed();

        self.device
            .write(&[addresses::PRESSURE_AND_TEMPERATURE_START])?;
        self.device.read(pressure_and_temperature.as_mut_bytes())?;

        Ok(pressure_and_temperature)
    }

    #[cfg(test)]
    fn get_config_reg(&mut self) -> ConfigReg {
        let mut buf = 0u8;

        self.device.write(&[addresses::CONFIG_ADDRESS]).unwrap();
        self.device.read(buf.as_mut_bytes()).unwrap();

        ConfigReg::new_with_raw_value(buf)
    }

    #[cfg(test)]
    fn get_ctrl_meas_reg(&mut self) -> CtrlMeasReg {
        let mut buf = 0u8;

        self.device.write(&[addresses::CTRL_MEAS_ADDRESS]).unwrap();
        self.device.read(buf.as_mut_bytes()).unwrap();

        CtrlMeasReg::new_with_raw_value(buf)
    }
}

mod addresses {
    /// Address of the first byte of trimming parameters
    pub const TRIMMING_PARAMETERS_START: u8 = 0x88;

    /// Address one past the last byte of trimming parameters
    pub const TRIMMING_PARAMETERS_END: u8 = 0xa0;

    pub const TRIMMING_PARAMETERS_LEN: u8 = TRIMMING_PARAMETERS_END - TRIMMING_PARAMETERS_START;

    pub const CTRL_MEAS_ADDRESS: u8 = 0xf4;
    pub const CONFIG_ADDRESS: u8 = 0xf5;

    /// Address of the first byte of pressure and temperature
    pub const PRESSURE_AND_TEMPERATURE_START: u8 = 0xf7;

    /// Address one past the last byte of pressure and temperature bytes
    pub const PRESSURE_AND_TEMPERATURE_END: u8 = 0xfd;

    pub const PRESSURE_AND_TEMPERATURE_LEN: u8 =
        PRESSURE_AND_TEMPERATURE_END - PRESSURE_AND_TEMPERATURE_START;
}

#[derive(
    Clone,
    Copy,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::Immutable,
    Default,
    Debug,
    PartialEq,
    Eq,
)]
struct TrimmingParameters {
    dig_t1: u16,
    dig_t2: i16,
    dig_t3: i16,

    dig_p1: u16,
    dig_p2: i16,
    dig_p3: i16,

    dig_p4: u16,
    dig_p5: i16,
    dig_p6: i16,

    dig_p7: u16,
    dig_p8: i16,
    dig_p9: i16,
}

const _: () =
    assert!(size_of::<TrimmingParameters>() == addresses::TRIMMING_PARAMETERS_LEN as usize);

#[derive(
    Clone,
    Copy,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::Immutable,
    Default,
    PartialEq,
    Eq,
    Debug,
)]
struct PressureAndTemperature {
    pub press_msb: u8,
    pub press_lsb: u8,
    pub press_xlsb: u8,

    pub temp_msb: u8,
    pub temp_lsb: u8,
    pub temp_xlsb: u8,
}

const _: () = assert!(
    size_of::<PressureAndTemperature>() == addresses::PRESSURE_AND_TEMPERATURE_LEN as usize
);

impl PressureAndTemperature {
    fn raw_pressure(&self) -> f64 {
        let combined = ((self.press_msb as u32) << 12)
            | ((self.press_lsb as u32) << 4)
            | ((self.press_xlsb as u32) >> 4);

        combined as f64
    }

    fn raw_temperature(&self) -> f64 {
        let combined = ((self.temp_msb as u32) << 12)
            | ((self.temp_lsb as u32) << 4)
            | ((self.temp_xlsb as u32) >> 4);

        combined as f64
    }

    fn pressure(&self, trimming_parameters: &TrimmingParameters, t_fine: f64) -> Pressure {
        // Bosch docs 3.12 Calculating pressure and temperature

        let dig_p1 = trimming_parameters.dig_p1 as f64;
        let dig_p2 = trimming_parameters.dig_p2 as f64;
        let dig_p3 = trimming_parameters.dig_p3 as f64;
        let dig_p4 = trimming_parameters.dig_p4 as f64;
        let dig_p5 = trimming_parameters.dig_p5 as f64;
        let dig_p6 = trimming_parameters.dig_p6 as f64;
        let dig_p7 = trimming_parameters.dig_p7 as f64;
        let dig_p8 = trimming_parameters.dig_p8 as f64;
        let dig_p9 = trimming_parameters.dig_p9 as f64;

        let raw_press = self.raw_pressure();
        let mut pressure = 1048576.0 - raw_press;

        let mut var1 = t_fine / 2.0 - 64000.0;
        let mut var2 = var1.powi(2) * dig_p6 / 32768.0;

        var2 += var1 * dig_p5 * 2.0;
        var2 = (var2 / 4.0) + dig_p4 * 65536.0;

        var1 = (dig_p3 * var1.powi(2) / 524288.0 + dig_p2 * var1) / 524288.0;
        var1 = (1.0 + var1 / 32768.0) * dig_p1;

        pressure = (pressure - var2 / 4096.0) * 6250.0 / var1;

        var1 = dig_p9 * pressure.powi(2) / 2147483648.0;
        var2 = pressure * dig_p8 / 32768.0;

        pressure += (var1 + var2 + dig_p7) / 16.0;

        Pressure::new::<pascal>(pressure)
    }

    fn temperature(
        &self,
        trimming_parameters: &TrimmingParameters,
    ) -> (f64, ThermodynamicTemperature) {
        // Bosch docs 3.12 Calculating pressure and temperature

        let temp = self.raw_temperature();
        let dig_t1 = trimming_parameters.dig_t1 as f64;
        let dig_t2 = trimming_parameters.dig_t2 as f64;
        let dig_t3 = trimming_parameters.dig_t3 as f64;

        let var1 = (temp / 16384.0 - (dig_t1 / 1024.0)) * dig_t2;
        let var2 = (temp / 131072.0 - dig_t1 / 8192.0).powi(2) * dig_t3;

        let t_fine = var1 + var2;
        let celsius = ThermodynamicTemperature::new::<degree_celsius>(t_fine / 5120.0);

        (t_fine, celsius)
    }

    #[cfg(test)]
    fn set_raw_tempeature(&mut self, raw_temperature: u32) {
        assert!(raw_temperature < (1 << 21));

        self.temp_xlsb = ((raw_temperature & 0x00_00_0f) as u8) << 4;
        self.temp_lsb = ((raw_temperature >> 4) & 0x00_00_ff) as u8;
        self.temp_msb = ((raw_temperature >> 12) & 0x00_00_ff) as u8;
    }

    #[cfg(test)]
    fn set_raw_pressure(&mut self, raw_pressure: u32) {
        assert!(raw_pressure < (1 << 21));

        self.press_xlsb = ((raw_pressure & 0x00_00_0f) as u8) << 4;
        self.press_lsb = ((raw_pressure >> 4) & 0x00_00_ff) as u8;
        self.press_msb = ((raw_pressure >> 12) & 0x00_00_ff) as u8;
    }
}

#[derive(Debug)]
#[bitenum(u2, exhaustive = true)]
enum PowerMode {
    Sleep = 0b00,
    // Two values correspond to forced (chapter 3.6 in Bosch docs)
    Forced = 0b01,
    ForcedAlt = 0b10,
    Normal = 0b11,
}

#[derive(Debug)]
#[bitenum(u3, exhaustive = true)]
enum Oversampling {
    Skipped = 0,
    X1 = 1,
    X2 = 2,
    X4 = 3,
    X8 = 4,
    // Three values correspond X16 oversampling (chapter 3.3.2 in Bosch docs)
    X16 = 5,
    X16Alt1 = 6,
    X16Alt2 = 7,
}

#[bitfield(u8, debug, default = 0)]
struct CtrlMeasReg {
    #[bits(0..=1, rw)]
    power_mode: PowerMode,
    #[bits(2..=4, rw)]
    pressure_oversampling: Oversampling,
    #[bits(5..=7, rw)]
    temperature_oversampling: Oversampling,
}

#[derive(Debug)]
#[bitenum(u3, exhaustive = true)]
enum Filter {
    Off = 0,
    X2 = 1,
    X4 = 2,
    X8 = 3,
    X16 = 4,

    _Unknown0 = 5,
    _Unknown1 = 6,
    _Unknown2 = 7,
}

#[bitfield(u8, debug, default = 0)]
struct ConfigReg {
    // Not really reserved, we just don't care about the value of these bits
    #[bits([0..=1, 5..=7], r)]
    reserved: u5,

    #[bits(2..=4, rw)]
    filter: Filter,
}

#[bitfield(u8, debug, default = 0)]
struct Status {
    #[bits([0..=2, 4..=7], r)]
    reserved: u7,

    #[bit(3, r)]
    measuring: bool,
}

#[cfg(test)]
mod tests {
    use i2cdev::mock::MockI2CDevice;

    use super::*;

    #[test]
    fn test_conversion_bosch_docs() {
        let trimming_params = TrimmingParameters {
            dig_t1: 27504,
            dig_t2: 26435,
            dig_t3: -1000,

            dig_p1: 36477,
            dig_p2: -10685,
            dig_p3: 3024,
            dig_p4: 2855,
            dig_p5: 140,
            dig_p6: -7,
            dig_p7: 15500,
            dig_p8: -14600,
            dig_p9: 6000,
        };

        let mut pressure_and_temperature = PressureAndTemperature::default();
        pressure_and_temperature.set_raw_tempeature(519888);
        pressure_and_temperature.set_raw_pressure(415148);

        let (t_fine, temperature) = pressure_and_temperature.temperature(&trimming_params);
        let pressure = pressure_and_temperature.pressure(&trimming_params, t_fine);

        assert_eq!(temperature.get::<degree_celsius>(), 25.08247793081682);
        assert_eq!(pressure.get::<pascal>(), 100653.26677582515);
    }

    #[test]
    fn test_new() {
        let mut device = MockI2CDevice::new();

        let mut expected_trimming_params = TrimmingParameters::new_zeroed();
        expected_trimming_params.as_mut_bytes().fill(0xaa);

        device.regmap.write_regs(
            addresses::TRIMMING_PARAMETERS_START as usize,
            expected_trimming_params.as_bytes(),
        );

        let mut bmp280 = Bmp280::new(device).unwrap();

        assert_eq!(bmp280.trimming_parameters, expected_trimming_params);
        assert_eq!(
            bmp280.get_config_reg().raw_value(),
            ConfigReg::default().with_filter(Filter::X16).raw_value()
        );
    }

    #[test]
    fn test_force_measurement() {
        let device = MockI2CDevice::new();

        let mut bmp280 = Bmp280::new(device).unwrap();

        let expected_ctrl_meas = CtrlMeasReg::builder()
            .with_power_mode(PowerMode::Forced)
            .with_pressure_oversampling(Oversampling::X16)
            .with_temperature_oversampling(Oversampling::X2)
            .build();

        bmp280.force_measurement().unwrap();

        assert_eq!(
            bmp280.get_ctrl_meas_reg().raw_value(),
            expected_ctrl_meas.raw_value()
        );
    }

    #[tokio::test]
    async fn test_pressure_and_temperature() {
        let mut device = MockI2CDevice::new();

        let mut expected_pressure_and_temperature = PressureAndTemperature::new_zeroed();
        expected_pressure_and_temperature.as_mut_bytes().fill(0xbb);

        device.regmap.write_regs(
            addresses::PRESSURE_AND_TEMPERATURE_START as usize,
            expected_pressure_and_temperature.as_bytes(),
        );

        let mut bmp280 = Bmp280::new(device).unwrap();

        let pressure_and_temperature = bmp280.get_pressure_and_temperature().unwrap();

        assert_eq!(pressure_and_temperature, expected_pressure_and_temperature);
    }
}
