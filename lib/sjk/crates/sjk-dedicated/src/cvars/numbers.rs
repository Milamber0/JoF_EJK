//! The C library's number reading and writing as `cvar.cpp` relies on it.

/// `strtod`: the value of the longest number at the start of `text` (after white
/// space) — decimal with an exponent, hexadecimal with a binary one, `inf`, `infinity`
/// or `nan` — and how many bytes it took; `(0.0, 0)` without one.
pub(super) fn strtod(text: &[u8]) -> (f64, usize) {
    let start = text
        .iter()
        .take_while(|byte| matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .count();
    let mut at = start;
    let negative = match text.get(at) {
        Some(b'-') => {
            at += 1;
            true
        }
        Some(b'+') => {
            at += 1;
            false
        }
        _ => false,
    };
    let sign = |value: f64| if negative { -value } else { value };
    let rest = &text[at..];
    let word =
        |word: &[u8]| rest.len() >= word.len() && rest[..word.len()].eq_ignore_ascii_case(word);
    if word(b"infinity") {
        return (sign(f64::INFINITY), at + 8);
    }
    if word(b"inf") {
        return (sign(f64::INFINITY), at + 3);
    }
    if word(b"nan") {
        let mut end = at + 3;
        // `nan(chars)`.
        if text.get(end) == Some(&b'(')
            && let Some(close) = text[end..].iter().position(|&byte| byte == b')')
            && text[end + 1..end + close]
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            end += close + 1;
        }
        return (sign(f64::NAN), end);
    }
    let hex = rest.len() > 2
        && rest[0] == b'0'
        && matches!(rest[1], b'x' | b'X')
        && (rest[2].is_ascii_hexdigit()
            || (rest[2] == b'.' && rest.get(3).is_some_and(u8::is_ascii_hexdigit)));
    if hex {
        let (mut value, mut end, mut scale) = (0.0_f64, at + 2, 0_i32);
        while let Some(digit) = text.get(end).and_then(|byte| (*byte as char).to_digit(16)) {
            value = value * 16.0 + f64::from(digit);
            end += 1;
        }
        if text.get(end) == Some(&b'.') {
            end += 1;
            while let Some(digit) = text.get(end).and_then(|byte| (*byte as char).to_digit(16)) {
                value = value * 16.0 + f64::from(digit);
                scale -= 4;
                end += 1;
            }
        }
        if matches!(text.get(end), Some(b'p' | b'P')) {
            let (exponent, used) = exponent(&text[end + 1..]);
            if used > 0 {
                scale = scale.saturating_add(exponent);
                end += 1 + used;
            }
        }
        return (sign(value * 2_f64.powi(scale)), end);
    }
    let digits = |from: usize| {
        text[from.min(text.len())..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count()
    };
    let whole = digits(at);
    let mut end = at + whole;
    let mut fraction = 0;
    if text.get(end) == Some(&b'.') {
        fraction = digits(end + 1);
        end += 1 + fraction;
    }
    if whole + fraction == 0 {
        return (0.0, 0);
    }
    if matches!(text.get(end), Some(b'e' | b'E')) {
        let (_, used) = exponent(&text[end + 1..]);
        if used > 0 {
            end += 1 + used;
        }
    }
    let number = std::str::from_utf8(&text[at..end])
        .ok()
        .and_then(|number| number.parse::<f64>().ok())
        .unwrap_or(0.0);
    (sign(number), end)
}

/// A signed decimal exponent and the bytes it took (0 without digits).
fn exponent(text: &[u8]) -> (i32, usize) {
    let sign = usize::from(matches!(text.first(), Some(b'+' | b'-')));
    let digits = text[sign..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 {
        return (0, 0);
    }
    let value = text[sign..sign + digits]
        .iter()
        .fold(0_i32, |value, digit| {
            value
                .saturating_mul(10)
                .saturating_add(i32::from(digit - b'0'))
        });
    (if text[0] == b'-' { -value } else { value }, sign + digits)
}

/// `atof`.
pub(super) fn atof(text: &[u8]) -> f64 {
    strtod(text).0
}

/// `atoi`, as glibc's `strtol` saturates and `int` truncates it.
pub(super) fn atoi(text: &[u8]) -> i32 {
    let start = text
        .iter()
        .take_while(|byte| matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .count();
    let text = &text[start..];
    let negative = text.first() == Some(&b'-');
    let text = &text[usize::from(matches!(text.first(), Some(b'+' | b'-')))..];
    let value = text
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .fold(0_i64, |value, digit| {
            if negative {
                value
                    .saturating_mul(10)
                    .saturating_sub(i64::from(digit - b'0'))
            } else {
                value
                    .saturating_mul(10)
                    .saturating_add(i64::from(digit - b'0'))
            }
        });
    value as i32
}

/// `Q_isanumber`: the whole text is one number, neither an overflow (`HUGE_VAL`) nor
/// out of range.
pub(super) fn is_a_number(text: &[u8]) -> bool {
    if text.is_empty() {
        return false;
    }
    let (value, used) = strtod(text);
    if value == f64::INFINITY {
        return false;
    }
    // `ERANGE`: a finite number too large or too small for a double.
    let literal_infinity = {
        let rest = text[text
            .iter()
            .take_while(|byte| byte.is_ascii_whitespace())
            .count()..]
            .strip_prefix(b"-")
            .unwrap_or_default();
        rest.len() >= 3 && rest[..3].eq_ignore_ascii_case(b"inf")
    };
    if (value.is_infinite() && !literal_infinity)
        || (value != 0.0 && value.abs() < f64::MIN_POSITIVE)
    {
        return false;
    }
    used == text.len()
}

/// `(int)` of a float as x86-64 converts it: out of range or not a number is `INT_MIN`.
pub(super) fn c_int(value: f32) -> i32 {
    if value.is_nan() || value >= 2_147_483_648.0 || value < -2_147_483_648.0 {
        i32::MIN
    } else {
        value as i32
    }
}

/// `Q_isintegral(value) ? "%i" : "%f"`, as `Cvar_SetValue` writes a float.
pub(super) fn format_value(value: f32) -> String {
    if c_int(value) as f32 == value {
        return c_int(value).to_string();
    }
    let wide = f64::from(value);
    if wide.is_nan() {
        return if wide.is_sign_negative() {
            "-nan".into()
        } else {
            "nan".into()
        };
    }
    if wide.is_infinite() {
        return if wide < 0.0 {
            "-inf".into()
        } else {
            "inf".into()
        };
    }
    format!("{wide:.6}")
}
