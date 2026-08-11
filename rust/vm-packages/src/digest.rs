use sha2::{Digest, Sha256};

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

#[cfg(test)]
mod tests {
    use super::sha256_hex;

    #[test]
    fn encodes_a_stable_sha256_digest() {
        assert_eq!(
            sha256_hex("vm"),
            "5bce98f73f3ed0c837f2729ed9509b38ea66a156db7f653356cb6fe37b366e85"
        );
    }
}
