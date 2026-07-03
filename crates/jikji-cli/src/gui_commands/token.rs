#[derive(Clone)]
pub(crate) struct ManagementToken(String);

impl ManagementToken {
    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }

    pub(crate) fn generate() -> jikji_core::Result<Self> {
        let mut bytes = [0_u8; 24];
        getrandom::fill(&mut bytes).map_err(|source| {
            jikji_core::io_error(
                "<gui-token>",
                std::io::Error::other(format!("management token generation failed: {source}")),
            )
        })?;
        Ok(Self(hex_encode(&bytes)))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn matches(&self, candidate: &str) -> bool {
        constant_time_eq(self.0.as_bytes(), candidate.as_bytes())
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let diff = left
        .iter()
        .zip(right.iter())
        .fold(0_u8, |acc, (left, right)| acc | (left ^ right));
    diff == 0
}
