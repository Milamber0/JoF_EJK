//! Conditions (`CSequencer::EvaluateConditional`, `Q3_Evaluate`).
//!
//! Each side of an `if` becomes text first — a number printed to three places, a vector
//! as three such numbers, a string as itself — with its type, and `Q3_Evaluate` reads the
//! text back to compare. A float compared with an int is compared as ints.

use crate::Icarus;
use crate::block::Block;
use crate::cnum::{format_f3, scan_f32, scan_i32, scan_vector, stricmp_equal};
use crate::host::{DebugLevel, IcarusHost, Owner};
use crate::ids::*;
use crate::print;

/// One side of a condition as text, where the reference keeps a pointer: its own
/// buffer, or the shared buffer a `get(STRING, ...)` answered in, read when compared.
enum Side {
    Text(String),
    Shared,
    Missing,
}

impl<O: Owner> Icarus<O> {
    /// `EvaluateConditional`: 1 if the `if` block's condition holds, 0 if not or if a
    /// side could not be read.
    pub(crate) fn evaluate_conditional<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        block: &Block,
        host: &mut H,
    ) -> i32 {
        let mut member = 0;
        let Some((first_type, first)) = self.condition_side(owner, block, &mut member, host) else {
            return 0;
        };
        let operator = block.member_id(member).unwrap_or(-1);
        member += 1;
        if !matches!(
            operator,
            TK_EQUALS | TK_GREATER_THAN | TK_LESS_THAN | TK_NOT
        ) {
            print::debug(
                host,
                DebugLevel::Error,
                "Invalid operator type found on conditional!\n",
            );
            return 0;
        }
        let Some((second_type, second)) = self.condition_side(owner, block, &mut member, host)
        else {
            return 0;
        };
        let resolve = |side: &Side, icarus: &Self| match side {
            Side::Text(text) => text.clone(),
            Side::Shared => icarus.shared.text().into_owned(),
            Side::Missing => String::new(),
        };
        let first = resolve(&first, self);
        let second = resolve(&second, self);
        evaluate(host, first_type, &first, second_type, &second, operator)
    }

    fn condition_side<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        block: &Block,
        member: &mut usize,
        host: &mut H,
    ) -> Option<(i32, Side)> {
        let id = block.member_id(*member).unwrap_or(-1);
        let at = *member;
        *member += 1;
        let vector_text = |value: [f32; 3]| {
            format!(
                "{} {} {}",
                format_f3(value[0]),
                format_f3(value[1]),
                format_f3(value[2])
            )
        };
        match id {
            TK_FLOAT => Some((id, Side::Text(format_f3(block.f32_at(at))))),
            TK_VECTOR => {
                let value = [
                    block.f32_at(*member),
                    block.f32_at(*member + 1),
                    block.f32_at(*member + 2),
                ];
                *member += 3;
                Some((id, Side::Text(vector_text(value))))
            }
            TK_STRING | TK_IDENTIFIER | TK_CHAR => {
                Some((id, Side::Text(block.str_at(at).into_owned())))
            }
            ID_GET => {
                let kind = block.f32_at(*member) as i32;
                let name = block.str_at(*member + 1).into_owned();
                *member += 2;
                let side = match kind {
                    TK_FLOAT => Side::Text(format_f3(self.ask_float(owner, kind, &name, host)?)),
                    TK_INT => {
                        Side::Text((self.ask_float(owner, kind, &name, host)? as i32).to_string())
                    }
                    TK_STRING => {
                        if !self.ask_string(owner, kind, &name, host) {
                            return None;
                        }
                        Side::Shared
                    }
                    TK_VECTOR => Side::Text(vector_text(
                        self.ask_vector(owner, kind, &name, [0.0; 3], host)?,
                    )),
                    _ => Side::Missing,
                };
                Some((kind, side))
            }
            ID_RANDOM => {
                let (min, max) = (block.f32_at(*member), block.f32_at(*member + 1));
                *member += 2;
                Some((TK_FLOAT, Side::Text(format_f3(host.random(min, max)))))
            }
            ID_TAG => {
                let name = block.str_at(*member).into_owned();
                let lookup = block.f32_at(*member + 1);
                *member += 2;
                let mut value = [0.0; 3];
                if !self.ask_tag(owner, &name, lookup as i32, &mut value, host) {
                    print::debug(
                        host,
                        DebugLevel::Error,
                        &format!("Unable to find tag \"{name}\"!\n"),
                    );
                    return None;
                }
                Some((TK_VECTOR, Side::Text(vector_text(value))))
            }
            _ => {
                print::debug(
                    host,
                    DebugLevel::Error,
                    "Invalid parameter type on conditional",
                );
                None
            }
        }
    }
}

/// `Q3_Evaluate`: 1 or 0.
fn evaluate<O: Owner, H: IcarusHost<O> + ?Sized>(
    host: &mut H,
    first_type: i32,
    first: &str,
    second_type: i32,
    second: &str,
    operator: i32,
) -> i32 {
    let (mut first_type, mut second_type) = (first_type, second_type);
    // "Always demote to int on float to integer comparisons"
    if (first_type == TK_FLOAT && second_type == TK_INT)
        || (first_type == TK_INT && second_type == TK_FLOAT)
    {
        first_type = TK_INT;
        second_type = TK_INT;
    }
    if first_type != second_type {
        print::debug(
            host,
            DebugLevel::Error,
            "Q3_Evaluate comparing two disimilar types!\n",
        );
        return 0;
    }
    enum Values {
        Float(f32, f32),
        Int(i32, i32),
        Vector([f32; 3], [f32; 3]),
        Text,
    }
    let values = match first_type {
        TK_FLOAT => Values::Float(
            scan_f32(first).unwrap_or(0.0),
            scan_f32(second).unwrap_or(0.0),
        ),
        TK_INT => Values::Int(scan_i32(first).unwrap_or(0), scan_i32(second).unwrap_or(0)),
        TK_VECTOR => {
            let (mut a, mut b) = ([0.0; 3], [0.0; 3]);
            scan_vector(first, &mut a);
            scan_vector(second, &mut b);
            Values::Vector(a, b)
        }
        TK_STRING | TK_IDENTIFIER => Values::Text,
        _ => {
            print::debug(
                host,
                DebugLevel::Warning,
                "Q3_Evaluate unknown type used!\n",
            );
            return 0;
        }
    };
    let equal_text = || stricmp_equal(first, second);
    let result = match (operator, values) {
        (TK_EQUALS, Values::Float(a, b)) => a == b,
        (TK_EQUALS, Values::Int(a, b)) => a == b,
        (TK_EQUALS, Values::Vector(a, b)) => a == b,
        (TK_EQUALS, Values::Text) => equal_text(),
        (TK_GREATER_THAN, Values::Float(a, b)) => a > b,
        (TK_GREATER_THAN, Values::Int(a, b)) => a > b,
        (TK_LESS_THAN, Values::Float(a, b)) => a < b,
        (TK_LESS_THAN, Values::Int(a, b)) => a < b,
        (TK_GREATER_THAN | TK_LESS_THAN, Values::Vector(..)) => {
            let which = if operator == TK_GREATER_THAN {
                "GREATER THAN"
            } else {
                "LESS THAN"
            };
            print::debug(
                host,
                DebugLevel::Error,
                &format!("Q3_Evaluate vector comparisons of type {which} cannot be performed!"),
            );
            false
        }
        (TK_GREATER_THAN | TK_LESS_THAN, Values::Text) => {
            let which = if operator == TK_GREATER_THAN {
                "GREATER THAN"
            } else {
                "LESS THAN"
            };
            print::debug(
                host,
                DebugLevel::Error,
                &format!("Q3_Evaluate string comparisons of type {which} cannot be performed!"),
            );
            false
        }
        (TK_NOT, Values::Float(a, b)) => a != b,
        (TK_NOT, Values::Int(a, b)) => a != b,
        (TK_NOT, Values::Vector(a, b)) => a != b,
        (TK_NOT, Values::Text) => !equal_text(),
        _ => {
            print::debug(
                host,
                DebugLevel::Error,
                "Q3_Evaluate unknown operator used!\n",
            );
            false
        }
    };
    i32::from(result)
}
