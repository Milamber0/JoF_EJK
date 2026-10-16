use sjk_protocol::MessageReader;

#[derive(Clone)]
pub(super) struct CommandText {
    bytes: [u8; 1024],
    len: usize,
}
impl Default for CommandText {
    fn default() -> Self {
        Self {
            bytes: [0; 1024],
            len: 0,
        }
    }
}
impl CommandText {
    pub fn set(&mut self, value: &[u8]) {
        let value = value.split(|&byte| byte == 0).next().unwrap_or_default();
        self.len = value.len().min(1023);
        self.bytes[..self.len].copy_from_slice(&value[..self.len]);
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
    pub fn read(reader: &mut MessageReader<'_>) -> Self {
        let mut text = Self::default();
        // MSG_ReadString consumes up to 1024 bytes but returns at most 1023:
        // the final protection NUL overwrites the last byte at the limit.
        for index in 0..1024 {
            let Ok(byte) = reader.read_u8() else {
                break;
            };
            if byte == 0 {
                break;
            }
            text.bytes[index] = if byte == b'%' { b'.' } else { byte };
            text.len = (index + 1).min(1023);
        }
        text
    }
}
