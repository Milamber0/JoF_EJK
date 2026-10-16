use crate::huffman_codes::HUFFMAN_CODES;
use std::error::Error;
use std::fmt;
use std::sync::OnceLock;

/// Commands sent inside a JKA server message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ServiceCommand {
    Bad = 0,
    Nop = 1,
    GameState = 2,
    ConfigString = 3,
    Baseline = 4,
    ServerCommand = 5,
    Download = 6,
    Snapshot = 7,
    SetGame = 8,
    MapChange = 9,
    End = 10,
}

impl TryFrom<u8> for ServiceCommand {
    type Error = MessageError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Bad),
            1 => Ok(Self::Nop),
            2 => Ok(Self::GameState),
            3 => Ok(Self::ConfigString),
            4 => Ok(Self::Baseline),
            5 => Ok(Self::ServerCommand),
            6 => Ok(Self::Download),
            7 => Ok(Self::Snapshot),
            8 => Ok(Self::SetGame),
            9 => Ok(Self::MapChange),
            10 => Ok(Self::End),
            _ => Err(MessageError::UnknownServiceCommand(value)),
        }
    }
}

/// Reads JKA's in-band message format.
///
/// Fields whose width is not divisible by eight begin with raw, low-order bits.
/// Every complete byte is then represented by the fixed Quake 3 Huffman tree.
pub struct MessageReader<'a> {
    data: &'a [u8],
    bit_position: usize,
}

impl<'a> MessageReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            bit_position: 0,
        }
    }

    pub fn read_bits(&mut self, width: u8) -> Result<u32, MessageError> {
        validate_width(width)?;

        let raw_bits = usize::from(width & 7);
        let mut value = 0_u32;
        for shift in 0..raw_bits {
            value |= u32::from(self.read_raw_bit()?) << shift;
        }

        let byte_bits = usize::from(width) - raw_bits;
        for shift in (0..byte_bits).step_by(8) {
            value |= u32::from(self.read_huffman_byte()?) << (shift + raw_bits);
        }
        Ok(value)
    }

    pub fn read_signed_bits(&mut self, width: u8) -> Result<i32, MessageError> {
        validate_width(width)?;
        let value = self.read_bits(width)?;
        if width == 32 {
            return Ok(value as i32);
        }

        let sign_bit = 1_u32 << (width - 1);
        if value & sign_bit == 0 {
            Ok(value as i32)
        } else {
            Ok((value | (!0_u32 << width)) as i32)
        }
    }

    pub fn read_u8(&mut self) -> Result<u8, MessageError> {
        Ok(self.read_bits(8)? as u8)
    }

    pub fn read_i16(&mut self) -> Result<i16, MessageError> {
        Ok(self.read_bits(16)? as u16 as i16)
    }

    pub fn read_i32(&mut self) -> Result<i32, MessageError> {
        Ok(self.read_bits(32)? as i32)
    }

    pub fn read_f32(&mut self) -> Result<f32, MessageError> {
        Ok(f32::from_bits(self.read_bits(32)?))
    }

    pub fn read_service_command(&mut self) -> Result<ServiceCommand, MessageError> {
        self.read_u8()?.try_into()
    }

    pub fn read_c_string(&mut self, maximum_bytes: usize) -> Result<Vec<u8>, MessageError> {
        let mut bytes = Vec::new();
        loop {
            let byte = self.read_u8()?;
            if byte == 0 {
                return Ok(bytes);
            }
            if bytes.len() == maximum_bytes {
                return Err(MessageError::StringTooLong { maximum_bytes });
            }
            bytes.push(byte);
        }
    }

    pub fn bit_position(&self) -> usize {
        self.bit_position
    }

    pub fn bits_remaining(&self) -> usize {
        self.data
            .len()
            .saturating_mul(8)
            .saturating_sub(self.bit_position)
    }

    fn read_raw_bit(&mut self) -> Result<u8, MessageError> {
        if self.bit_position >= self.data.len().saturating_mul(8) {
            return Err(MessageError::UnexpectedEnd {
                bit_position: self.bit_position,
                total_bits: self.data.len().saturating_mul(8),
            });
        }

        let byte = self.data[self.bit_position >> 3];
        let bit = (byte >> (self.bit_position & 7)) & 1;
        self.bit_position += 1;
        Ok(bit)
    }

    fn read_huffman_byte(&mut self) -> Result<u8, MessageError> {
        let tree = decoder_tree();
        let mut node_index = 0;
        loop {
            let node = &tree[node_index];
            if let Some(symbol) = node.symbol {
                return Ok(symbol);
            }

            let bit = usize::from(self.read_raw_bit()?);
            node_index = node.children[bit].ok_or(MessageError::InvalidHuffmanCode)?;
        }
    }
}

/// Writes the same compressed bitstream format used by OpenJK's `MSG_WriteBits`.
pub struct MessageWriter {
    data: Vec<u8>,
    bit_position: usize,
    maximum_bytes: usize,
}

impl MessageWriter {
    pub fn new(maximum_bytes: usize) -> Self {
        Self {
            data: Vec::new(),
            bit_position: 0,
            maximum_bytes,
        }
    }

    pub fn write_bits(&mut self, value: u32, width: u8) -> Result<(), MessageError> {
        validate_width(width)?;
        let mut remaining = if width == 32 {
            value
        } else {
            value & ((1_u32 << width) - 1)
        };

        let raw_bits = width & 7;
        for _ in 0..raw_bits {
            self.write_raw_bit((remaining & 1) as u8)?;
            remaining >>= 1;
        }

        let byte_bits = width - raw_bits;
        for _ in (0..byte_bits).step_by(8) {
            self.write_huffman_byte((remaining & 0xff) as u8)?;
            remaining >>= 8;
        }
        Ok(())
    }

    pub fn write_u8(&mut self, value: u8) -> Result<(), MessageError> {
        self.write_bits(u32::from(value), 8)
    }

    pub fn write_i16(&mut self, value: i16) -> Result<(), MessageError> {
        self.write_bits(value as u16 as u32, 16)
    }

    pub fn write_i32(&mut self, value: i32) -> Result<(), MessageError> {
        self.write_bits(value as u32, 32)
    }

    pub fn write_f32(&mut self, value: f32) -> Result<(), MessageError> {
        self.write_bits(value.to_bits(), 32)
    }

    pub fn write_c_string(&mut self, value: &[u8]) -> Result<(), MessageError> {
        for byte in value {
            self.write_u8(*byte)?;
        }
        self.write_u8(0)
    }

    /// Produces OpenJK-compatible storage, including its trailing padding byte
    /// when the final bit position lies exactly on a byte boundary.
    pub fn finish(mut self) -> Result<Vec<u8>, MessageError> {
        if self.bit_position == 0 {
            return Ok(Vec::new());
        }
        let legacy_length = (self.bit_position >> 3) + 1;
        if legacy_length > self.maximum_bytes {
            return Err(MessageError::CapacityExceeded {
                maximum_bytes: self.maximum_bytes,
            });
        }
        self.data.resize(legacy_length, 0);
        Ok(self.data)
    }

    /// Like [`Self::finish`], but keeps the writer and its storage for the next
    /// message: a server sends one per client per frame and must not allocate each.
    pub fn finished(&mut self) -> Result<&[u8], MessageError> {
        if self.bit_position == 0 {
            return Ok(&[]);
        }
        let legacy_length = (self.bit_position >> 3) + 1;
        if legacy_length > self.maximum_bytes {
            return Err(MessageError::CapacityExceeded {
                maximum_bytes: self.maximum_bytes,
            });
        }
        self.data.resize(legacy_length, 0);
        Ok(&self.data)
    }

    /// Start a new message in the same storage.
    pub fn clear(&mut self) {
        self.data.clear();
        self.bit_position = 0;
    }

    pub fn bit_position(&self) -> usize {
        self.bit_position
    }

    fn write_raw_bit(&mut self, bit: u8) -> Result<(), MessageError> {
        let byte_index = self.bit_position >> 3;
        if byte_index >= self.maximum_bytes {
            return Err(MessageError::CapacityExceeded {
                maximum_bytes: self.maximum_bytes,
            });
        }
        if byte_index == self.data.len() {
            self.data.push(0);
        }
        self.data[byte_index] |= (bit & 1) << (self.bit_position & 7);
        self.bit_position += 1;
        Ok(())
    }

    fn write_huffman_byte(&mut self, byte: u8) -> Result<(), MessageError> {
        let (code, length) = HUFFMAN_CODES[usize::from(byte)];
        for bit_index in 0..length {
            self.write_raw_bit(((code >> bit_index) & 1) as u8)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct DecoderNode {
    children: [Option<usize>; 2],
    symbol: Option<u8>,
}

fn decoder_tree() -> &'static [DecoderNode] {
    static TREE: OnceLock<Vec<DecoderNode>> = OnceLock::new();
    TREE.get_or_init(|| {
        let mut nodes = vec![DecoderNode::default()];
        for (symbol, (code, length)) in HUFFMAN_CODES.iter().copied().enumerate() {
            let mut node_index = 0;
            for bit_index in 0..length {
                let bit = usize::from(((code >> bit_index) & 1) != 0);
                let child = if let Some(child) = nodes[node_index].children[bit] {
                    child
                } else {
                    let child = nodes.len();
                    nodes.push(DecoderNode::default());
                    nodes[node_index].children[bit] = Some(child);
                    child
                };
                node_index = child;
            }
            debug_assert!(nodes[node_index].symbol.is_none());
            debug_assert!(nodes[node_index].children == [None, None]);
            nodes[node_index].symbol = Some(symbol as u8);
        }
        nodes
    })
}

fn validate_width(width: u8) -> Result<(), MessageError> {
    if !(1..=32).contains(&width) {
        return Err(MessageError::InvalidBitWidth(width));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MessageError {
    InvalidBitWidth(u8),
    UnexpectedEnd {
        bit_position: usize,
        total_bits: usize,
    },
    InvalidHuffmanCode,
    UnknownServiceCommand(u8),
    StringTooLong {
        maximum_bytes: usize,
    },
    CapacityExceeded {
        maximum_bytes: usize,
    },
}

impl fmt::Display for MessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBitWidth(width) => write!(formatter, "invalid message bit width {width}"),
            Self::UnexpectedEnd {
                bit_position,
                total_bits,
            } => write!(
                formatter,
                "message ended at bit {bit_position} of {total_bits}"
            ),
            Self::InvalidHuffmanCode => formatter.write_str("invalid Huffman code in message"),
            Self::UnknownServiceCommand(command) => {
                write!(formatter, "unknown server message command {command}")
            }
            Self::StringTooLong { maximum_bytes } => {
                write!(formatter, "message string exceeds {maximum_bytes} bytes")
            }
            Self::CapacityExceeded { maximum_bytes } => {
                write!(formatter, "message exceeds {maximum_bytes} bytes")
            }
        }
    }
}

impl Error for MessageError {}
