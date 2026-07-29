//! 酷狗 KRC 解密。

use std::io::Read;

use flate2::read::ZlibDecoder;

const KRC_MAGIC: &[u8; 4] = b"krc1";
const KRC_XOR_KEY: [u8; 16] = [
    0x40, 0x47, 0x61, 0x77, 0x5e, 0x32, 0x74, 0x47, 0x51, 0x36, 0x31, 0x2d, 0xce, 0xd2, 0x6e, 0x69,
];

pub fn decrypt_krc(data: &[u8]) -> Result<String, String> {
    let encrypted = data
        .strip_prefix(KRC_MAGIC)
        .ok_or_else(|| "invalid KRC header".to_string())?;
    let decrypted = encrypted
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ KRC_XOR_KEY[index % KRC_XOR_KEY.len()])
        .collect::<Vec<_>>();
    let mut decoder = ZlibDecoder::new(decrypted.as_slice());
    let mut decoded = Vec::new();
    decoder
        .read_to_end(&mut decoded)
        .map_err(|error| format!("KRC zlib decode failed: {error}"))?;
    String::from_utf8(decoded).map_err(|error| format!("KRC utf8 decode failed: {error}"))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::ZlibEncoder;

    use super::{KRC_MAGIC, KRC_XOR_KEY, decrypt_krc};

    #[test]
    fn decrypts_krc_payload() {
        let plaintext = "[1000,1000]<0,500,0>你<500,500,0>好";
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(plaintext.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut fixture = KRC_MAGIC.to_vec();
        fixture.extend(
            compressed
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ KRC_XOR_KEY[index % KRC_XOR_KEY.len()]),
        );

        assert_eq!(decrypt_krc(&fixture).unwrap(), plaintext);
    }
}
