//! Script variables (`Q3_Registers.cpp`): names a script `declare`s, shared by every
//! script on the server, as floats, strings or vectors (a vector is kept as its text,
//! "Work around for vector types").

use std::collections::BTreeMap;

use crate::cnum::scan_vector;
use crate::ids::{TK_FLOAT, TK_STRING, TK_VECTOR};

/// The reference's cap on declared variables (`MAX_VARIABLES`): `declare` refuses a
/// variable once more than this many exist. It is the default of
/// [`crate::IcarusConfig::max_variables`], not a limit of this crate.
pub const MAX_VARIABLES: i32 = 32;

/// What a name is declared as (`VTYPE_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariableType {
    /// Not declared (`VTYPE_NONE`).
    None = 0,
    /// A float (`VTYPE_FLOAT`).
    Float = 1,
    /// A string (`VTYPE_STRING`).
    String = 2,
    /// A vector (`VTYPE_VECTOR`).
    Vector = 3,
}

/// Why a declaration was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeclareError {
    TooMany,
    UnknownType,
}

/// Every declared variable.
#[derive(Debug)]
pub struct Variables {
    strings: BTreeMap<String, String>,
    floats: BTreeMap<String, f32>,
    vectors: BTreeMap<String, String>,
    count: i32,
    limit: i32,
}

impl Variables {
    /// No variables; `limit` as [`MAX_VARIABLES`].
    pub fn new(limit: i32) -> Self {
        Self {
            strings: BTreeMap::new(),
            floats: BTreeMap::new(),
            vectors: BTreeMap::new(),
            count: 0,
            limit,
        }
    }

    /// The cap `declare` checks.
    pub fn limit(&self) -> i32 {
        self.limit
    }

    /// `Q3_VariableDeclared`: strings first, then floats, then vectors.
    pub fn declared(&self, name: &str) -> VariableType {
        if self.strings.contains_key(name) {
            VariableType::String
        } else if self.floats.contains_key(name) {
            VariableType::Float
        } else if self.vectors.contains_key(name) {
            VariableType::Vector
        } else {
            VariableType::None
        }
    }

    /// `Q3_DeclareVariable`: a name already declared is left alone.
    pub(crate) fn declare(&mut self, kind: i32, name: &str) -> Result<(), DeclareError> {
        if self.declared(name) != VariableType::None {
            return Ok(());
        }
        if self.count > self.limit {
            return Err(DeclareError::TooMany);
        }
        match kind {
            TK_FLOAT => {
                self.floats.insert(name.to_owned(), 0.0);
            }
            TK_STRING => {
                self.strings.insert(name.to_owned(), "NULL".to_owned());
            }
            TK_VECTOR => {
                self.vectors
                    .insert(name.to_owned(), "0.0 0.0 0.0".to_owned());
            }
            _ => return Err(DeclareError::UnknownType),
        }
        self.count += 1;
        Ok(())
    }

    /// `Q3_FreeVariable`.
    pub fn free(&mut self, name: &str) {
        if self.strings.remove(name).is_some()
            || self.floats.remove(name).is_some()
            || self.vectors.remove(name).is_some()
        {
            self.count -= 1;
        }
    }

    /// `Q3_GetFloatVariable`.
    pub fn float(&self, name: &str) -> Option<f32> {
        self.floats.get(name).copied()
    }

    /// `Q3_GetStringVariable`.
    pub fn string(&self, name: &str) -> Option<&str> {
        self.strings.get(name).map(String::as_str)
    }

    /// `Q3_GetVectorVariable`: the vector's text read back as three floats.
    pub fn vector(&self, name: &str) -> Option<[f32; 3]> {
        self.vectors.get(name).map(|text| {
            let mut value = [0.0; 3];
            scan_vector(text, &mut value);
            value
        })
    }

    /// `Q3_SetFloatVariable`.
    pub fn set_float(&mut self, name: &str, value: f32) {
        if let Some(slot) = self.floats.get_mut(name) {
            *slot = value;
        }
    }

    /// `Q3_SetStringVariable`.
    pub fn set_string(&mut self, name: &str, value: &str) {
        if let Some(slot) = self.strings.get_mut(name) {
            value.clone_into(slot);
        }
    }

    /// `Q3_SetVectorVariable`.
    pub fn set_vector(&mut self, name: &str, value: &str) {
        if let Some(slot) = self.vectors.get_mut(name) {
            value.clone_into(slot);
        }
    }

    /// `Q3_InitVariables`: forget every variable; returns how many were left.
    pub fn clear(&mut self) -> i32 {
        self.strings.clear();
        self.floats.clear();
        self.vectors.clear();
        std::mem::take(&mut self.count)
    }

    /// Every variable, for tests and diagnostics: floats, then strings, then vectors,
    /// each in name order.
    pub fn dump(&self) -> impl Iterator<Item = (VariableType, &str, String)> {
        let floats = self.floats.iter().map(|(name, value)| {
            (
                VariableType::Float,
                name.as_str(),
                format!("{:08x}", value.to_bits()),
            )
        });
        let strings = self
            .strings
            .iter()
            .map(|(name, value)| (VariableType::String, name.as_str(), value.clone()));
        let vectors = self
            .vectors
            .iter()
            .map(|(name, value)| (VariableType::Vector, name.as_str(), value.clone()));
        floats.chain(strings).chain(vectors)
    }
}
