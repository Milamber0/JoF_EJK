//! JoF EJK's free-choice cosmetics: the hat and cape a player wears, carried
//! after the saber colour in `color1` and `color2`.
//!
//! JoF EJK (`cg_local.h`, `CG_NewClientInfo`) writes the cosmetic's name
//! straight after the colour digits: `color1 "8santahat"` is saber colour 8
//! wearing the hat `santahat`, and `color2` holds the cape the same way.
//! Servers copy the keys through to the `c1`/`c2` clientinfo verbatim (cut
//! to 15 bytes by the 16-byte buffers of OpenJK and JA+ gamecode) and every
//! client reads the colour with `atoi`, which stops at the first letter, so
//! the name needs no server support and an unaware client sees only the
//! colour. A name is at most [`MAX_COSMETIC_NAME`] bytes (`MAX_COSMETIC_LENGTH`
//! 14 with its terminator) and must not start with a digit, which the
//! receiving `atoi` would swallow.

/// Longest cosmetic name, in bytes.
pub const MAX_COSMETIC_NAME: usize = 13;

/// The two places a cosmetic is worn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CosmeticSlot {
    /// A hat, on the `*head_top` bolt, carried by `color1`/`c1`.
    Hat,
    /// A cape, on the `*back` bolt, carried by `color2`/`c2`.
    Cape,
}

impl CosmeticSlot {
    /// Both slots, hat first.
    pub const ALL: [Self; 2] = [Self::Hat, Self::Cape];

    /// Position in [`Self::ALL`], which is also the saber the key colours.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The userinfo cvar carrying this slot.
    pub const fn cvar(self) -> &'static str {
        match self {
            Self::Hat => "color1",
            Self::Cape => "color2",
        }
    }

    /// The clientinfo key carrying this slot.
    pub const fn clientinfo_key(self) -> &'static str {
        match self {
            Self::Hat => "c1",
            Self::Cape => "c2",
        }
    }
}

/// Split a `color1`/`color2` (or `c1`/`c2`) value into the colour `atoi`
/// reads and the cosmetic name after the leading digits (`Q_StripDigits`
/// with `REMOVE_DIGITS_INITIAL`), or `None` when there is no valid name.
pub fn split_color_value(value: &str) -> (i64, Option<&str>) {
    let trimmed = value.trim_start();
    let (negative, unsigned) = match trimmed.as_bytes().first() {
        Some(b'-') => (true, &trimmed[1..]),
        Some(b'+') => (false, &trimmed[1..]),
        _ => (false, trimmed),
    };
    let digits = unsigned
        .bytes()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    let magnitude = unsigned[..digits].bytes().fold(0_i64, |total, byte| {
        total
            .saturating_mul(10)
            .saturating_add(i64::from(byte - b'0'))
    });
    let colour = if negative { -magnitude } else { magnitude };
    // `Q_StripDigits` drops only the digits before the first non-digit.
    let leading = value
        .bytes()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    let name = &value[leading..];
    (colour, valid_cosmetic_name(name).then_some(name))
}

/// Whether `name` can be worn: 1 to [`MAX_COSMETIC_NAME`] bytes of letters,
/// digits, `_` and `-`, not starting with a digit, and not `none`.
pub fn valid_cosmetic_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_COSMETIC_NAME
        && !name.as_bytes()[0].is_ascii_digit()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        && !name.eq_ignore_ascii_case("none")
}

/// What a player's clientinfo (`CS_PLAYERS + n`) says they wear in `slot`.
pub fn worn_cosmetic(clientinfo: &[u8], slot: CosmeticSlot) -> Option<&str> {
    let value = crate::LegacyClientInfo::new(clientinfo).text(slot.clientinfo_key())?;
    split_color_value(value).1
}

/// `color1`/`color2` for saber colour `colour` wearing `cosmetic` (when it
/// is a valid name), as JoF EJK's `UI_SetCosmetic` writes it.
pub fn join_color_value(colour: i64, cosmetic: Option<&str>) -> String {
    match cosmetic.filter(|name| valid_cosmetic_name(name)) {
        Some(name) => format!("{colour}{name}"),
        None => colour.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_and_name_split_as_atoi_and_strip_digits_do() {
        assert_eq!(split_color_value("8santahat"), (8, Some("santahat")));
        assert_eq!(split_color_value("4"), (4, None));
        assert_eq!(split_color_value("6royalcape"), (6, Some("royalcape")));
        assert_eq!(split_color_value(""), (0, None));
        assert_eq!(split_color_value("fedora"), (0, Some("fedora")));
        // Digits after the first letter belong to the name.
        assert_eq!(split_color_value("3fedora2"), (3, Some("fedora2")));
        assert_eq!(split_color_value("4none"), (4, None));
        assert_eq!(split_color_value("4a long name!"), (4, None));
        assert_eq!(split_color_value("4thisnameistoolong"), (4, None));
    }

    #[test]
    fn names_join_back_to_the_same_value() {
        assert_eq!(join_color_value(4, Some("santahat")), "4santahat");
        assert_eq!(join_color_value(4, None), "4");
        assert_eq!(join_color_value(6, Some("bad name")), "6");
        for value in ["8santahat", "4", "6predatorhelm"] {
            let (colour, name) = split_color_value(value);
            assert_eq!(join_color_value(colour, name), value);
        }
        // Two colour digits and the longest name fit OpenJK's 16-byte buffer.
        assert!(join_color_value(11, Some("predatorhelm1")).len() <= 15);
    }

    #[test]
    fn clientinfo_names_the_worn_pieces() {
        let info = br"n\Sol\t\0\c1\4santahat\c2\0";
        assert_eq!(worn_cosmetic(info, CosmeticSlot::Hat), Some("santahat"));
        assert_eq!(worn_cosmetic(info, CosmeticSlot::Cape), None);
        assert_eq!(worn_cosmetic(br"n\Sol", CosmeticSlot::Hat), None);
    }

    #[test]
    fn names_follow_the_userinfo_rules() {
        assert!(valid_cosmetic_name("santahat"));
        assert!(valid_cosmetic_name("fedora_2"));
        assert!(!valid_cosmetic_name("2fedora"));
        assert!(!valid_cosmetic_name("hat\\x"));
        assert!(!valid_cosmetic_name("hat;"));
        assert!(!valid_cosmetic_name("None"));
        assert!(!valid_cosmetic_name(""));
    }
}
