use std::error::Error;
use std::fmt;
use std::io::{self, Read};

pub const MAX_LEGACY_MESSAGE_BYTES: usize = 49_152;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DemoRecord {
    pub sequence: i32,
    pub payload: Vec<u8>,
}

/// Streaming reader for JKA `.dm_25` and `.dm_26` record framing.
///
/// Each record is an i32 little-endian sequence number, an i32 little-endian
/// payload length, and the compressed server message. A length of -1 ends the
/// stream.
pub struct DemoReader<R> {
    input: R,
    maximum_message_bytes: usize,
    ended: bool,
    record_index: u64,
}

impl<R: Read> DemoReader<R> {
    pub fn new(input: R) -> Self {
        Self {
            input,
            maximum_message_bytes: MAX_LEGACY_MESSAGE_BYTES,
            ended: false,
            record_index: 0,
        }
    }

    pub fn with_maximum_message_bytes(input: R, maximum_message_bytes: usize) -> Self {
        Self {
            input,
            maximum_message_bytes,
            ended: false,
            record_index: 0,
        }
    }

    pub fn next_record(&mut self) -> Result<Option<DemoRecord>, DemoError> {
        if self.ended {
            return Ok(None);
        }

        let Some(sequence) = read_optional_i32(&mut self.input)? else {
            self.ended = true;
            return Ok(None);
        };
        let length = read_required_i32(&mut self.input, "demo record length")?;
        if length == -1 {
            self.ended = true;
            return Ok(None);
        }
        if length < 0 {
            return Err(DemoError::InvalidMessageLength {
                record_index: self.record_index,
                length,
            });
        }

        let length = length as usize;
        if length > self.maximum_message_bytes {
            return Err(DemoError::MessageTooLarge {
                record_index: self.record_index,
                length,
                maximum: self.maximum_message_bytes,
            });
        }

        let mut payload = vec![0; length];
        read_exact_context(&mut self.input, &mut payload, "demo message payload")?;
        self.record_index += 1;
        Ok(Some(DemoRecord { sequence, payload }))
    }

    pub fn record_index(&self) -> u64 {
        self.record_index
    }
}

fn read_optional_i32(input: &mut impl Read) -> Result<Option<i32>, DemoError> {
    let mut bytes = [0; 4];
    let first = loop {
        match input.read(&mut bytes[..1]) {
            Ok(0) => return Ok(None),
            Ok(1) => break bytes[0],
            Ok(_) => unreachable!("one-byte read returned more than one byte"),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(DemoError::Io(error)),
        }
    };
    bytes[0] = first;
    read_exact_context(input, &mut bytes[1..], "demo record sequence")?;
    Ok(Some(i32::from_le_bytes(bytes)))
}

fn read_required_i32(input: &mut impl Read, context: &'static str) -> Result<i32, DemoError> {
    let mut bytes = [0; 4];
    read_exact_context(input, &mut bytes, context)?;
    Ok(i32::from_le_bytes(bytes))
}

fn read_exact_context(
    input: &mut impl Read,
    mut output: &mut [u8],
    context: &'static str,
) -> Result<(), DemoError> {
    let expected = output.len();
    let mut actual = 0;
    while !output.is_empty() {
        match input.read(output) {
            Ok(0) => {
                return Err(DemoError::Truncated {
                    context,
                    expected,
                    actual,
                });
            }
            Ok(count) => {
                actual += count;
                output = &mut output[count..];
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(DemoError::Io(error)),
        }
    }
    Ok(())
}

#[derive(Debug)]
pub enum DemoError {
    Io(io::Error),
    Truncated {
        context: &'static str,
        expected: usize,
        actual: usize,
    },
    InvalidMessageLength {
        record_index: u64,
        length: i32,
    },
    MessageTooLarge {
        record_index: u64,
        length: usize,
        maximum: usize,
    },
}

impl fmt::Display for DemoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "failed to read demo: {error}"),
            Self::Truncated {
                context,
                expected,
                actual,
            } => write!(
                formatter,
                "truncated {context}: expected {expected} bytes, received {actual}"
            ),
            Self::InvalidMessageLength {
                record_index,
                length,
            } => write!(
                formatter,
                "demo record {record_index} has invalid message length {length}"
            ),
            Self::MessageTooLarge {
                record_index,
                length,
                maximum,
            } => write!(
                formatter,
                "demo record {record_index} is {length} bytes, exceeding the {maximum}-byte limit"
            ),
        }
    }
}

impl Error for DemoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for DemoError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
