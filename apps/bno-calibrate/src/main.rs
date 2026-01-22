use bno_055::BNO_055_I2C_ADDR;
use chrono::{Datelike, Timelike, Utc};
use i2cdev::linux::LinuxI2CDevice;
use std::time::Duration;

fn main() {
    reset_bno();
    std::thread::sleep(Duration::from_secs(1));

    let i2c = LinuxI2CDevice::new("/dev/i2c-1", BNO_055_I2C_ADDR as u16).unwrap();
    let mut bno = bno_055::Bno055::new(i2c).unwrap();

    println!("Connected to BNO");

    bno.set_operating_mode(bno_055::OperatingMode::NDOF)
        .unwrap();

    println!("Calibrate BNO according to the instructions in BOSCH docs");

    let mut i = 0;
    loop {
        let calibration_status = bno.get_calibration_status().unwrap();

        println!("({i:04}) {:?}", calibration_status);

        if let (3, 3, 3, 3) = (
            calibration_status.mag_status().value(),
            calibration_status.acc_status().value(),
            calibration_status.gyr_status().value(),
            calibration_status.sys_status().value(),
        ) {
            let sensor_config = bno.get_sensor_config().unwrap();

            let path = format!("bno_sensor_config.{}.json", now_as_string());
            let contents = serde_json::to_string_pretty(&sensor_config).unwrap();
            println!("{contents}");

            std::fs::write(&path, contents).unwrap();
            println!("Saved sensor config to {path}");

            break;
        }

        i += 1;
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn reset_bno() {
    println!("Resetting BNO...");
    let i2c = LinuxI2CDevice::new("/dev/i2c-1", BNO_055_I2C_ADDR as u16).unwrap();
    let bno = bno_055::Bno055::new(i2c).unwrap();
    bno.reset().unwrap();
}

fn now_as_string() -> String {
    let now = Utc::now();
    let (year, month, day, hour, minute, second) = (
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    );

    format!("{year}{month}{day}{hour}{minute}{second}")
}
