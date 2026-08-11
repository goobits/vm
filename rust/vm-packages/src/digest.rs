use sha2::{Digest, Sha256};
use std::io::{self, Read};

pub fn sha256_hex(value: impl AsRef<[u8]>) -> String {
    encode_hex(Sha256::digest(value.as_ref()))
}

pub fn encode_hex(value: impl AsRef<[u8]>) -> String {
    use std::fmt::Write as _;

    let value = value.as_ref();
    value.iter().fold(
        String::with_capacity(value.len() * 2),
        |mut encoded, byte| {
            write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
            encoded
        },
    )
}

pub fn sha256_reader(mut reader: impl Read) -> io::Result<(String, u64)> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size = 0_u64;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok((encode_hex(hasher.finalize()), size))
}

#[cfg(test)]
mod tests {
    use super::{sha256_hex, sha256_reader};

    #[test]
    fn encodes_a_stable_sha256_digest() {
        assert_eq!(
            sha256_hex("vm"),
            "5bce98f73f3ed0c837f2729ed9509b38ea66a156db7f653356cb6fe37b366e85"
        );
    }

    #[test]
    fn hashes_streams_without_buffering_the_whole_input() {
        let (digest, size) = sha256_reader(std::io::Cursor::new(b"vm")).unwrap();
        assert_eq!(digest, sha256_hex("vm"));
        assert_eq!(size, 2);
    }
}
