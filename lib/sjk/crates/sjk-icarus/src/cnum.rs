//! C's number text, as the interpreter's C writes and reads it: `printf`'s `%f` and
//! `%.3f`, `atof`, `sscanf`'s `%f` and `%d`. Scripts carry numbers as text between the
//! interpreter and the game (a `set` hands over `"1.000000 2.000000 3.000000"`), and
//! conditions compare numbers printed to three places, so the text must be C's.
//!
//! `sscanf("%f")` rounds the decimal text to a float once (`strtof`); `atof` rounds it to
//! a double and the caller then to a float — the two can differ in the last bit, and
//! the reference uses both.

/// `printf("%.*f", precision, value)` of a float promoted to double.
pub fn format_fixed(value: f32, precision: usize) -> String {
    if value.is_nan() {
        return if value.is_sign_negative() {
            "-nan".into()
        } else {
            "nan".into()
        };
    }
    if value.is_infinite() {
        return if value < 0.0 {
            "-inf".into()
        } else {
            "inf".into()
        };
    }
    format!("{:.*}", precision, f64::from(value))
}

/// `printf("%f", value)`.
pub fn format_f(value: f32) -> String {
    format_fixed(value, 6)
}

/// `printf("%.3f", value)`.
pub fn format_f3(value: f32) -> String {
    format_fixed(value, 3)
}

/// `printf("%d", (int) value)`: C's conversion truncates toward zero.
pub fn float_to_int(value: f32) -> i32 {
    value as i32
}

/// The decimal number at the start of `text` after white space: its text and the bytes
/// it used, or `None` if there is none. Hexadecimal numbers, `inf` and `nan` are not read.
fn decimal_prefix(text: &str) -> Option<(&str, usize)> {
    let bytes = text.as_bytes();
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let digits = |from: usize| {
        bytes[from.min(bytes.len())..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count()
    };
    let mut end = start + usize::from(matches!(bytes.get(start), Some(b'+' | b'-')));
    let whole = digits(end);
    end += whole;
    let mut fraction = 0;
    if bytes.get(end) == Some(&b'.') {
        fraction = digits(end + 1);
        end += 1 + fraction;
    }
    if whole + fraction == 0 {
        return None;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(bytes.get(end + 1), Some(b'+' | b'-')));
        let exponent = digits(end + 1 + sign);
        if exponent > 0 {
            end += 1 + sign + exponent;
        }
    }
    Some((&text[start..end], end))
}

/// `atof`: the leading number rounded to a double (zero without one).
pub fn atof(text: &str) -> f64 {
    decimal_prefix(text)
        .and_then(|(number, _)| number.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// `atoi`: the leading integer (zero without one), saturated as glibc's `strtol` and
/// then truncated to an int.
pub fn atoi(text: &str) -> i32 {
    int_prefix(text).map_or(0, |(value, _)| value)
}

fn int_prefix(text: &str) -> Option<(i32, usize)> {
    let bytes = text.as_bytes();
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let negative = bytes.get(start) == Some(&b'-');
    let digits_from = start + usize::from(matches!(bytes.get(start), Some(b'+' | b'-')));
    let count = bytes[digits_from.min(bytes.len())..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if count == 0 {
        return None;
    }
    let magnitude = bytes[digits_from..digits_from + count]
        .iter()
        .fold(0_i128, |value, byte| {
            (value * 10 + i128::from(byte - b'0')).min(i128::from(i64::MAX) + 1)
        });
    let value = if negative { -magnitude } else { magnitude };
    let long = value.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
    Some((long as i32, digits_from + count))
}

/// `sscanf(text, "%f", ...)`: one float, or `None` if the text holds none.
pub fn scan_f32(text: &str) -> Option<f32> {
    decimal_prefix(text).and_then(|(number, _)| number.parse::<f32>().ok())
}

/// `sscanf(text, "%d", ...)`: one int, or `None`.
pub fn scan_i32(text: &str) -> Option<i32> {
    int_prefix(text).map(|(value, _)| value)
}

/// `sscanf(text, "%f %f %f", ...)`: the floats read before the first that fails, into
/// `out` (the rest left as they were), and how many there were.
pub fn scan_vector(text: &str, out: &mut [f32; 3]) -> usize {
    let mut rest = text;
    for (index, slot) in out.iter_mut().enumerate() {
        let Some((number, used)) = decimal_prefix(rest) else {
            return index;
        };
        let Ok(value) = number.parse::<f32>() else {
            return index;
        };
        *slot = value;
        rest = &rest[used..];
    }
    3
}

/// `Q_stricmp(a, b) == 0`: equal when only ASCII letters differ in case.
pub fn stricmp_equal(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}
