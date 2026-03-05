//! https://m5stack.oss-cn-shenzhen.aliyuncs.com/resource/docs/products/module/Module-LoRa433_V1.1/sx1278.pdf
use std::io::{self};

use arbitrary_int::{u2, u3, u4};
use bitbybit::{bitenum, bitfield};
use spidev::{SpiModeFlags, Spidev, SpidevOptions, SpidevTransfer};
use zerocopy::byteorder::big_endian;

mod addresses {
    pub const FIFO: u8 = 0x00;
    pub const OP_MODE: u8 = 0x01;

    pub const CARRIER_FREQUENCY_MSB: u8 = 0x06;
    pub const PA_CONFIG: u8 = 0x09;

    pub const FIFO_PTR: u8 = 0x0d;
    pub const FIFO_TX_BASE: u8 = 0x0e;
    #[expect(dead_code)]
    pub const FIFO_RX_BASE: u8 = 0x0f;
    #[expect(dead_code)]
    pub const FIFO_RX_CURRENT_ADDR: u8 = 0x10;

    #[expect(dead_code)]
    pub const LAST_RX_BYTES: u8 = 0x13;

    pub const MODEM_CONFIG_1: u8 = 0x1D;
    pub const MODEM_CONFIG_2: u8 = 0x1E;

    pub const PAYLOAD_LENGTH: u8 = 0x22;

    pub const SYNC_WORD: u8 = 0x39;

    pub const VERSION: u8 = 0x42;
}

pub struct Sx1276 {
    device: Spidev,
}

impl Sx1276 {
    pub fn new(mut spidev: Spidev) -> io::Result<Sx1276> {
        let options = SpidevOptions::new()
            .bits_per_word(8)
            .max_speed_hz(20_000)
            .mode(SpiModeFlags::SPI_MODE_1)
            .build();

        spidev.configure(&options)?;

        let mut sx1276 = Self { device: spidev };

        sx1276.set_device_mode(DeviceMode::Sleep)?;

        sx1276.set_op_mode(
            RegOpMode::builder()
                .with_long_range_mode(LongRangeMode::LoRa)
                .with_frequency_mode(FrequencyMode::HighFrequencyMode)
                .with_device_mode(DeviceMode::Sleep)
                .build(),
        )?;

        sx1276.set_frequency(CarrierFrequency::mhz868())?;
        sx1276.set_modem_config1(
            RegModemConfig1::builder()
                .with_bandwidth(BandWidth::Khz250)
                .with_coding_rate(CodingRate::Cr4_5)
                .with_header_mode(HeaderMode::Explicit)
                .build(),
        )?;

        sx1276.set_modem_config2(
            RegModemConfig2::builder()
                .with_spreading_factor(SpreadingFactor::SF11)
                .with_tx_mode(TxMode::Normal)
                .with_payload_crc(false)
                .with_symb_timeout_msb(u2::new(0u8))
                .build(),
        )?;

        sx1276.set_sync_word(0x12)?;
        sx1276.set_pa_config(
            RegPaConfig::builder()
                .with_pa_select(PaSelect::PaBoost)
                .with_max_power(u3::new(0)) // irrelevant due to PaBoost
                .with_output_power(u4::new(12))
                .build(),
        )?;
        sx1276.set_register(addresses::FIFO_TX_BASE, 0x00)?;

        Ok(sx1276)
    }

    fn get_register(&mut self, address: u8) -> io::Result<u8> {
        let mut buf = [address, 0x0];

        let mut transfer = SpidevTransfer::read_write_in_place(&mut buf);
        self.device.transfer(&mut transfer)?;

        Ok(buf[1])
    }

    fn set_register(&mut self, address: u8, value: u8) -> io::Result<()> {
        let buf = [address | 0x80, value];
        let mut transfer = SpidevTransfer::write(&buf);
        self.device.transfer(&mut transfer)
    }

    fn set_pa_config(&mut self, pa_config: RegPaConfig) -> io::Result<()> {
        self.set_register(addresses::PA_CONFIG, pa_config.raw_value())
    }

    fn set_frequency(&mut self, frequency: CarrierFrequency) -> io::Result<()> {
        let frequency = frequency.register_bytes();
        let buf = [
            addresses::CARRIER_FREQUENCY_MSB | 0x80,
            frequency[0],
            frequency[1],
            frequency[2],
        ];
        let mut transfer = SpidevTransfer::write(&buf);
        self.device.transfer(&mut transfer)
    }

    fn set_payload_length(&mut self, len: u8) -> io::Result<()> {
        self.set_register(addresses::PAYLOAD_LENGTH, len)
    }

    pub fn set_sync_word(&mut self, sync_word: u8) -> io::Result<()> {
        self.set_register(addresses::SYNC_WORD, sync_word)
    }

    fn set_op_mode(&mut self, new_op_mode: RegOpMode) -> io::Result<()> {
        self.set_register(addresses::OP_MODE, new_op_mode.raw_value())
    }

    fn get_op_mode(&mut self) -> io::Result<RegOpMode> {
        self.get_register(addresses::OP_MODE)
            .map(RegOpMode::new_with_raw_value)
    }

    fn reset_fifo_ptr(&mut self) -> io::Result<()> {
        let new_ptr = self.get_register(addresses::FIFO_TX_BASE)?;

        self.set_register(addresses::FIFO_PTR, new_ptr)
    }

    fn set_device_mode(&mut self, new_device_mode: DeviceMode) -> io::Result<()> {
        let current_op_mode = self.get_op_mode()?;
        self.set_op_mode(current_op_mode.with_device_mode(new_device_mode))
    }

    fn write_to_fifo(&mut self, payload: &[u8]) -> io::Result<()> {
        let mut transfers = [
            SpidevTransfer::write(&[addresses::FIFO | 0x80]),
            SpidevTransfer::write(payload),
        ];

        self.device.transfer_multiple(&mut transfers)
    }

    pub fn send(&mut self, payload: &[u8]) -> io::Result<()> {
        const FIFO_SIZE: usize = 255;
        let trimmed_payload = payload.get(0..FIFO_SIZE).unwrap_or(payload);

        self.set_device_mode(DeviceMode::Standby)?;
        self.reset_fifo_ptr()?;
        self.write_to_fifo(trimmed_payload)?;
        self.set_payload_length(trimmed_payload.len() as u8)?;

        self.set_device_mode(DeviceMode::Tx)
    }

    pub fn get_silicon_version(&mut self) -> io::Result<u8> {
        self.get_register(addresses::VERSION)
    }

    fn set_modem_config1(&mut self, modem_config1: RegModemConfig1) -> io::Result<()> {
        self.set_register(addresses::MODEM_CONFIG_1, modem_config1.raw_value())
    }

    fn set_modem_config2(&mut self, modem_config2: RegModemConfig2) -> io::Result<()> {
        self.set_register(addresses::MODEM_CONFIG_2, modem_config2.raw_value())
    }
}

#[derive(Debug, PartialEq, Eq)]
#[bitenum(u1, exhaustive = true)]
enum LongRangeMode {
    FskOok = 0,
    LoRa = 1,
}

#[derive(Debug, PartialEq, Eq)]
#[bitenum(u1, exhaustive = true)]
enum FrequencyMode {
    HighFrequencyMode = 0,
    LowFrequencyMode = 1,
}

#[derive(Debug, PartialEq, Eq)]
#[bitenum(u3, exhaustive = true)]
enum DeviceMode {
    Sleep = 0,
    Standby = 1,
    FsTx = 2,
    Tx = 3,
    FsRx = 4,
    RxContinuous = 5,
    RxSingle = 6,
    Cad = 7,
}

#[bitfield(u8, debug, default = 0)]
struct RegOpMode {
    #[bit(7, rw)]
    long_range_mode: LongRangeMode,

    #[bits(4..=6, r)]
    reserved: u3,

    #[bit(3, rw)]
    frequency_mode: FrequencyMode,

    #[bits(0..=2, rw)]
    device_mode: DeviceMode,
}

#[derive(Debug, PartialEq, Eq)]
#[bitenum(u1, exhaustive = true)]
enum PaSelect {
    Rfo = 0,
    PaBoost = 1,
}

#[bitfield(u8, debug, default = 0)]
struct RegPaConfig {
    #[bit(7, rw)]
    pa_select: PaSelect,

    // Ignored if pa_select == PaBoost
    #[bits(4..=6, rw)]
    max_power: u3,

    #[bits(0..=3, rw)]
    output_power: u4,
}

#[derive(Debug)]
#[bitenum(u4, exhaustive = false)]
enum BandWidth {
    Khz7_8 = 0b0000,
    Khz10_4 = 0b0001,
    Khz15_6 = 0b0010,
    Khz20_8 = 0b0011,
    Khz31_25 = 0b0100,
    Khz41_7 = 0b0101,
    Khz62_5 = 0b0110,
    Khz125 = 0b0111,
    Khz250 = 0b1000,
    Khz500 = 0b1001,
}

#[derive(Debug)]
#[bitenum(u3, exhaustive = false)]
enum CodingRate {
    Cr4_5 = 0b001,
    Cr4_6 = 0b010,
    Cr4_7 = 0b011,
    Cr4_8 = 0b100,
}

#[derive(Debug)]
#[bitenum(u1, exhaustive = true)]
enum HeaderMode {
    Explicit = 0,
    Implicit = 1,
}

#[bitfield(u8, debug, default = 0)]
struct RegModemConfig1 {
    #[bits(4..=7, rw)]
    bandwidth: Option<BandWidth>,

    #[bits(1..=3, rw)]
    coding_rate: Option<CodingRate>,

    #[bit(0, rw)]
    header_mode: HeaderMode,
}

#[derive(Debug)]
#[bitenum(u4, exhaustive = false)]
enum SpreadingFactor {
    SF6 = 6,
    SF7 = 7,
    SF8 = 8,
    SF9 = 9,
    SF10 = 10,
    SF11 = 11,
    SF12 = 12,
}

#[derive(Debug)]
#[bitenum(u1, exhaustive = true)]
enum TxMode {
    Normal = 0,
    Continuous = 1,
}

#[bitfield(u8, debug, default = 0)]
struct RegModemConfig2 {
    #[bits(4..=7, rw)]
    spreading_factor: Option<SpreadingFactor>,

    #[bit(3, rw)]
    tx_mode: TxMode,

    #[bit(2, rw)]
    payload_crc: bool,

    #[bits(0..=1, rw)]
    symb_timeout_msb: u2,
}

struct CarrierFrequency {
    register_value: big_endian::U32,
}

impl CarrierFrequency {
    const fn from_hz(hz: f64) -> Self {
        let resolution_hz = 61.035_f64;

        let register_value = (hz / resolution_hz) as u32;

        let register_value = big_endian::U32::new(register_value);

        Self { register_value }
    }

    const fn mhz868() -> Self {
        Self::from_hz(868_000_000_f64)
    }

    /// Returns big endian bytes
    fn register_bytes(&self) -> [u8; 3] {
        self.register_value.to_bytes()[1..4].try_into().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use crate::CarrierFrequency;

    #[test]
    fn mhz434_bytes() {
        let bytes = CarrierFrequency::from_hz(434_000_000f64).register_bytes();

        assert_eq!(bytes, [0x6c, 0x80, 0x12]);
    }

    #[test]
    fn mhz868_bytes() {
        let bytes = CarrierFrequency::from_hz(868_000_000_f64).register_bytes();

        assert_eq!(bytes, [0xd9, 0x00, 0x24]);
    }
}
