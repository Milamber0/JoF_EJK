//! Owned parsing for Quake 3/Jedi Academy entity dictionaries.

use std::error::Error;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entity {
    fields: Vec<(String, String)>,
}

impl Entity {
    /// An entity built from pairs rather than parsed from a lump: a game spawning one
    /// itself, or a value a lump's quoting cannot express.
    pub fn from_fields(fields: Vec<(String, String)>) -> Self {
        Self { fields }
    }

    pub fn fields(&self) -> &[(String, String)] {
        &self.fields
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .rev()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }

    pub fn classname(&self) -> Option<&str> {
        self.get("classname")
    }

    pub fn vector(&self, key: &str) -> Result<Option<[f32; 3]>, EntityError> {
        let Some(value) = self.get(key) else {
            return Ok(None);
        };
        let components = value
            .split_ascii_whitespace()
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| EntityError::InvalidVector {
                key: key.to_owned(),
                value: value.to_owned(),
            })?;
        let components: [f32; 3] =
            components
                .try_into()
                .map_err(|_| EntityError::InvalidVector {
                    key: key.to_owned(),
                    value: value.to_owned(),
                })?;
        if components.iter().all(|component| component.is_finite()) {
            Ok(Some(components))
        } else {
            Err(EntityError::InvalidVector {
                key: key.to_owned(),
                value: value.to_owned(),
            })
        }
    }

    pub fn number(&self, key: &str) -> Result<Option<f32>, EntityError> {
        let Some(value) = self.get(key) else {
            return Ok(None);
        };
        let number = value
            .parse::<f32>()
            .map_err(|_| EntityError::InvalidNumber {
                key: key.to_owned(),
                value: value.to_owned(),
            })?;
        if number.is_finite() {
            Ok(Some(number))
        } else {
            Err(EntityError::InvalidNumber {
                key: key.to_owned(),
                value: value.to_owned(),
            })
        }
    }
}

pub fn parse_entity_lump(bytes: &[u8]) -> Result<Vec<Entity>, EntityError> {
    let mut lexer = Lexer { bytes, cursor: 0 };
    let mut entities = Vec::new();
    while let Some(token) = lexer.next()? {
        if token != Token::OpeningBrace {
            return Err(lexer.syntax("expected opening brace"));
        }
        let mut fields = Vec::new();
        loop {
            match lexer.next()? {
                Some(Token::ClosingBrace) => break,
                Some(Token::Text(key)) => match lexer.next()? {
                    Some(Token::Text(value)) => fields.push((key, value)),
                    _ => return Err(lexer.syntax("expected quoted value after entity key")),
                },
                Some(Token::OpeningBrace) => {
                    return Err(lexer.syntax("unexpected opening brace inside entity"));
                }
                None => return Err(lexer.syntax("unterminated entity")),
            }
        }
        entities.push(Entity { fields });
    }
    Ok(entities)
}

#[derive(Debug, Eq, PartialEq)]
enum Token {
    OpeningBrace,
    ClosingBrace,
    Text(String),
}

struct Lexer<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl Lexer<'_> {
    fn next(&mut self) -> Result<Option<Token>, EntityError> {
        self.skip_trivia();
        let Some(&byte) = self.bytes.get(self.cursor) else {
            return Ok(None);
        };
        match byte {
            0 => {
                self.cursor = self.bytes.len();
                Ok(None)
            }
            b'{' => {
                self.cursor += 1;
                Ok(Some(Token::OpeningBrace))
            }
            b'}' => {
                self.cursor += 1;
                Ok(Some(Token::ClosingBrace))
            }
            b'"' => self.quoted().map(|value| Some(Token::Text(value))),
            _ => Err(self.syntax("expected brace or quoted string")),
        }
    }

    fn skip_trivia(&mut self) {
        loop {
            while self
                .bytes
                .get(self.cursor)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.cursor += 1;
            }
            if self.bytes.get(self.cursor..self.cursor + 2) == Some(b"//") {
                self.cursor += 2;
                while self
                    .bytes
                    .get(self.cursor)
                    .is_some_and(|byte| *byte != b'\n')
                {
                    self.cursor += 1;
                }
                continue;
            }
            break;
        }
    }

    fn quoted(&mut self) -> Result<String, EntityError> {
        self.cursor += 1;
        let mut output = Vec::new();
        while let Some(&byte) = self.bytes.get(self.cursor) {
            self.cursor += 1;
            match byte {
                b'"' => {
                    return String::from_utf8(output)
                        .map_err(|_| self.syntax("entity string is not UTF-8"));
                }
                b'\\' => {
                    let escaped = *self
                        .bytes
                        .get(self.cursor)
                        .ok_or_else(|| self.syntax("unterminated escape sequence"))?;
                    self.cursor += 1;
                    output.push(escaped);
                }
                0 => return Err(self.syntax("unterminated quoted string")),
                _ => output.push(byte),
            }
        }
        Err(self.syntax("unterminated quoted string"))
    }

    fn syntax(&self, message: &'static str) -> EntityError {
        EntityError::Syntax {
            offset: self.cursor,
            message,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntityError {
    Syntax {
        offset: usize,
        message: &'static str,
    },
    InvalidVector {
        key: String,
        value: String,
    },
    InvalidNumber {
        key: String,
        value: String,
    },
}

impl fmt::Display for EntityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax { offset, message } => {
                write!(formatter, "entity syntax error at byte {offset}: {message}")
            }
            Self::InvalidVector { key, value } => {
                write!(
                    formatter,
                    "entity field {key:?} is not a finite 3-vector: {value:?}"
                )
            }
            Self::InvalidNumber { key, value } => {
                write!(
                    formatter,
                    "entity field {key:?} is not a finite number: {value:?}"
                )
            }
        }
    }
}

impl Error for EntityError {}
