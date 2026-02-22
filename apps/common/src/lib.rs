use chacha20poly1305::{ChaCha20Poly1305, KeyInit as _, Nonce, aead::Aead as _};
use zerocopy::IntoBytes as _;

#[derive(zerocopy::IntoBytes, zerocopy::Immutable, zerocopy::FromBytes, Default, Debug)]
pub struct RadioMsg {
    pub timestamp: f64,
    pub latitude_degrees: f64,
    pub longitude_degrees: f64,
    pub course_over_ground_degrees: f64,
    pub speed_over_ground_meters_per_second: f64,
    pub altitude_meters: f64,
    pub satellites: u64,
}

impl RadioMsg {
    pub fn encrypt(&self, counter: u32, key: &[u8; 32]) -> Vec<u8> {
        let cipher = ChaCha20Poly1305::new(key.into());

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[..4].copy_from_slice(&counter.to_le_bytes());
        let nonce = Nonce::from(nonce_bytes);

        let encrypted = cipher.encrypt(&nonce, self.as_bytes()).unwrap();
        let mut full = Vec::from(counter.to_le_bytes());
        full.extend_from_slice(&encrypted);

        full
    }

    pub fn decrypt(raw_msg: &[u8], key: &[u8; 32]) -> Option<(u32, RadioMsg)> {
        let cipher = ChaCha20Poly1305::new(key.into());

        if raw_msg.len() != size_of::<u32>() + size_of::<RadioMsg>() + 16 {
            println!("Bad message length");
            return None;
        }

        let counter = {
            let mut tmp = 0u32;
            tmp.as_mut_bytes()
                .copy_from_slice(&raw_msg[..size_of::<u32>()]);

            tmp
        };

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[..4].copy_from_slice(&counter.to_le_bytes());
        let nonce = Nonce::from(nonce_bytes);

        let bytes = cipher
            .decrypt(&nonce, &raw_msg[size_of::<u32>()..])
            .inspect_err(|e| println!("Failed to decrypt: {e}"))
            .ok()?;

        let decrypted_msg = {
            let mut tmp = RadioMsg::default();
            tmp.as_mut_bytes().copy_from_slice(&bytes);
            tmp
        };

        Some((counter, decrypted_msg))
    }
}
