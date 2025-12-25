use rustix::{fd::AsFd, fs::StatVfs};
use std::{
    io::SeekFrom,
    num::{IntErrorKind, ParseIntError},
};
use tokio::{
    fs::File,
    io::{self, AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, BufReader},
};
use uom::si::{
    f64::{Information, ThermodynamicTemperature},
    information::{byte, kibibyte},
    thermodynamic_temperature::degree_celsius,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MemoryUsageInfo {
    pub total: Information,
    pub free: Information,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilesystemUsageInfo {
    pub total: Information,
    pub free: Information,
}

fn parse_temperature(temperature_str: &str) -> Result<ThermodynamicTemperature, ParseIntError> {
    temperature_str
        .trim_end()
        .parse::<usize>()
        // Linux gives out temperature in milldegrees celsius
        .map(|val| ThermodynamicTemperature::new::<degree_celsius>(val as f64 / 1000.0_f64))
}

fn calculate_filesystem_usage(stat: &StatVfs) -> FilesystemUsageInfo {
    let block_size = Information::new::<byte>(stat.f_bsize as f64);

    let blocks_free = stat.f_bfree as f64;
    let blocks_total = stat.f_blocks as f64;

    FilesystemUsageInfo {
        total: blocks_total * block_size,
        free: blocks_free * block_size,
    }
}

fn parse_meminfo_line(line: &str) -> Result<Information, IntErrorKind> {
    // Each line of /proc/meminfo looks like this
    // SomeKey:   SomeValue [kB]
    // For example:
    // ```
    // MemTotal:       64866396 kB
    // ```
    // Notably, kB meas kibibyte here ((https://docs.redhat.com/en/documentation/red_hat_enterprise_linux/6/html/deployment_guide/s2-proc-meminfo))
    //
    // While not all lines are in kB, this function assumes they are
    let mut parts = line.split_whitespace();

    parts.next(); // Skipping the key like: "MemTotal:" or "MemFree:"

    let value_str = parts.next().ok_or(IntErrorKind::Empty)?;

    let value = value_str.parse::<usize>().map_err(|err| *err.kind())?;

    Ok(Information::new::<kibibyte>(value as f64))
}

#[derive(Debug)]
pub struct CpuTemperature {
    cpu_temp_file: File,
}

impl CpuTemperature {
    pub fn new(cpu_temp_file: File) -> Self {
        Self { cpu_temp_file }
    }

    pub async fn read(&mut self) -> io::Result<ThermodynamicTemperature> {
        self.cpu_temp_file.seek(SeekFrom::Start(0)).await?;

        let mut temperature = String::with_capacity(12);
        self.cpu_temp_file.read_to_string(&mut temperature).await?;

        parse_temperature(&temperature).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
    }
}

pub struct FileSystemUsage {
    file_on_fs: File,
}

impl FileSystemUsage {
    pub fn new(file_on_fs: File) -> Self {
        Self { file_on_fs }
    }

    pub fn get(&self) -> io::Result<FilesystemUsageInfo> {
        rustix::fs::fstatvfs(self.file_on_fs.as_fd())
            .map(|stat| calculate_filesystem_usage(&stat))
            .map_err(std::io::Error::from)
    }
}

pub struct MemoryUsage {
    meminfo_file: File,
}

impl MemoryUsage {
    pub fn new(meminfo_file: File) -> Self {
        Self { meminfo_file }
    }

    pub async fn read(&mut self) -> io::Result<MemoryUsageInfo> {
        self.meminfo_file.seek(SeekFrom::Start(0)).await?;

        // We use very small capcacity, because the first two meminfo lines are likely to be less
        // than 80 bytes
        let mut buf_reader = BufReader::with_capacity(80, &mut self.meminfo_file);

        let mut line = String::with_capacity(40);

        let mem_total = {
            buf_reader.read_line(&mut line).await?;
            parse_meminfo_line(&line).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?
        };
        line.clear();

        let mem_free = {
            buf_reader.read_line(&mut line).await?;
            parse_meminfo_line(&line).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?
        };
        line.clear();

        Ok(MemoryUsageInfo {
            total: mem_total,
            free: mem_free,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use rustix::fs::StatVfsMountFlags;

    use crate::*;

    #[test]
    fn test_parse_temperature() {
        let temperature = "29250";

        assert_eq!(
            parse_temperature(temperature).unwrap(),
            uom::si::f64::ThermodynamicTemperature::new::<
                uom::si::thermodynamic_temperature::degree_celsius,
            >(29.25)
        );
    }

    #[test]
    fn test_parse_temperature_newline() {
        let temperature = "45750\n";

        assert_eq!(
            parse_temperature(temperature).unwrap(),
            uom::si::f64::ThermodynamicTemperature::new::<
                uom::si::thermodynamic_temperature::degree_celsius,
            >(45.75)
        );
    }

    #[test]
    fn test_parse_temperature_non_num() {
        let trash = "foo bar baz";
        parse_temperature(trash).unwrap_err();
    }

    #[test]
    fn test_parse_temperature_non_integer() {
        let num = "77000.34";

        parse_temperature(num).unwrap_err();
    }

    #[tokio::test]
    async fn test_cpu_temperature_read() {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(b"90525").unwrap();

        let mut cpu_temp = CpuTemperature::new(File::from_std(file));

        assert_eq!(
            cpu_temp.read().await.unwrap(),
            uom::si::f64::ThermodynamicTemperature::new::<
                uom::si::thermodynamic_temperature::degree_celsius,
            >(90.525)
        );
        assert_eq!(
            cpu_temp.read().await.unwrap(),
            uom::si::f64::ThermodynamicTemperature::new::<
                uom::si::thermodynamic_temperature::degree_celsius,
            >(90.525)
        );
        assert_eq!(
            cpu_temp.read().await.unwrap(),
            uom::si::f64::ThermodynamicTemperature::new::<
                uom::si::thermodynamic_temperature::degree_celsius,
            >(90.525)
        );
    }

    #[test]
    fn test_calculate_filesystem_usage() {
        let dummy = 0xf00;

        let stat = StatVfs {
            f_bsize: 4096,
            f_frsize: dummy,
            f_blocks: 956,
            f_bfree: 712,
            f_bavail: dummy,
            f_files: dummy,
            f_ffree: dummy,
            f_favail: dummy,
            f_fsid: dummy,
            f_flag: StatVfsMountFlags::empty(),
            f_namemax: dummy,
        };

        let usage = calculate_filesystem_usage(&stat);

        assert_eq!(usage.free, Information::new::<byte>(2916352_f64));
        assert_eq!(usage.total, Information::new::<byte>(3915776_f64));
    }

    #[test]
    fn test_parse_meminfo() {
        let input = "MemTotal:       64866396 kB";

        parse_meminfo_line(input).unwrap();
    }

    #[test]
    fn test_parse_meminfo_non_int() {
        let input = "MemTotal:       6486.6396 kB";

        parse_meminfo_line(input).unwrap_err();
    }

    #[test]
    fn test_parse_meminfo_non_num() {
        let input = "MemTotal:       foo kB";

        parse_meminfo_line(input).unwrap_err();
    }

    #[test]
    fn test_parse_meminfo_invalid_line() {
        let input = "foo";

        parse_meminfo_line(input).unwrap_err();
    }

    #[tokio::test]
    async fn test_memory_usage() {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(b"MemTotal:       64866384 kB\nMemFree:        34343068 kB\n")
            .unwrap();

        let mut memory_usage = MemoryUsage::new(File::from_std(file));

        assert_eq!(
            memory_usage.read().await.unwrap(),
            MemoryUsageInfo {
                total: uom::si::f64::Information::new::<kibibyte>(64866384_f64),
                free: uom::si::f64::Information::new::<kibibyte>(34343068_f64),
            }
        );

        assert_eq!(
            memory_usage.read().await.unwrap(),
            MemoryUsageInfo {
                total: uom::si::f64::Information::new::<kibibyte>(64866384_f64),
                free: uom::si::f64::Information::new::<kibibyte>(34343068_f64),
            }
        );
    }
}
