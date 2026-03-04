use std::io;
use std::io::Read;
use std::io::Write;
use std::{error::Error, time::Duration};

use common::RadioMsg;
use serialport::TTYPort;

fn main() -> Result<(), Box<dyn Error>> {
    let key = std::env::var("LORA_ENCRYPTION_KEY")
        .expect("LORA_ENCRYPTION_KEY envirnoment variable must set to a 32-byte long string");

    let key: [u8; 32] = key
        .as_bytes()
        .try_into()
        .expect("Key must be exactly 32 bytes long");

    let ports = serialport::available_ports().expect("No ports found!");
    let port = ports.first().unwrap();

    println!("Opening {}", port.port_name);
    let port = serialport::new(&port.port_name, 9600)
        .timeout(Duration::from_secs(1))
        .open_native()
        .unwrap();

    let mut tty = TtyAdapter::new(port);

    tty.write("AT+MODE=TEST\r\n").unwrap();
    println!("{}", tty.read_line().unwrap().trim_end());

    tty.write("AT+TEST=RXLRPKT\r\n").unwrap();
    println!("{}", tty.read_line().unwrap().trim_end());

    loop {
        let line = tty.read_line().unwrap();
        println!("{}", line.trim_end());

        if line.starts_with("+TEST: RX \"") {
            match parse_data(line.trim_end()) {
                Err(e) => println!("Failed to parse data: {e}"),
                Ok(msg) => {
                    if let Some((timestamp, msg)) = RadioMsg::decrypt(&msg, &key) {
                        println!("{timestamp}: {msg:?}")
                    }
                }
            }
        };
    }
}

fn parse_data(s: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let rx = s
        .strip_prefix("+TEST: RX \"")
        .ok_or("bad RX prefix")?
        .strip_suffix('"')
        .ok_or("bad RX suffix")?;
    let mut data = Vec::new();
    for chunk in rx.as_bytes().chunks_exact(2) {
        data.push(u8::from_str_radix(std::str::from_utf8(chunk)?, 16)?);
    }
    Ok(data)
}

struct TtyAdapter {
    tty: TTYPort,
    len: usize,
    buf: [u8; 1000],
}

impl TtyAdapter {
    pub fn new(tty: TTYPort) -> Self {
        Self {
            tty,
            len: 0,
            buf: [0u8; 1000],
        }
    }

    fn read_next_batch(&mut self) -> io::Result<Option<String>> {
        const CRLF: &[u8] = b"\r\n";

        if let Some(position) = memchr::memmem::find(&self.buf[..self.len], CRLF) {
            let line_length = position + CRLF.len();
            let line = String::from_utf8_lossy(&self.buf[..line_length]).to_string();

            self.buf.copy_within(line_length..self.len, 0);

            self.len -= line_length;

            Ok(Some(line))
        } else {
            let new_bytes_count = self.tty.read(&mut self.buf[self.len..])?;

            self.len += new_bytes_count;
            Ok(None)
        }
    }

    pub fn read_line(&mut self) -> io::Result<String> {
        loop {
            let batch = match self.read_next_batch() {
                Ok(batch) => Ok(batch),
                Err(e) if e.kind() == io::ErrorKind::TimedOut => Ok(None),
                Err(e) => Err(e),
            };

            match batch? {
                None => continue,
                Some(line) => return Ok(line),
            }
        }
    }

    pub fn write(&mut self, command: &str) -> io::Result<()> {
        println!("Writing: {command}");
        self.tty.write_all(command.as_bytes())?;
        Ok(())
    }
}
