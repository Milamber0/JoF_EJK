//! The debug panel's built-in test list: `assets/debug_panel.txt`, parsed once when
//! the console starts. The file header documents the format; [`parse`] enforces it,
//! and the tests below check the embedded file so a broken edit fails `cargo test`.

use std::fmt;

/// The test list built into the client.
pub(super) const EMBEDDED: &str = include_str!("../assets/debug_panel.txt");

/// Areas an entry may name.
pub(super) const AREAS: [&str; 7] = [
    "Console & chat",
    "Input",
    "Menus & settings",
    "HUD",
    "Rendering & effects",
    "Audio",
    "Gameplay",
];

/// Longest value, in bytes; longer lines would end in an ellipsis on narrow windows.
pub(super) const LINE_LIMIT: usize = 96;

/// One change in the build and how to test it.
#[derive(Debug)]
pub(super) struct Entry {
    /// Stable key of the entry's tick.
    pub(super) id: String,
    pub(super) area: String,
    pub(super) title: String,
    pub(super) changes: Vec<String>,
    pub(super) tests: Vec<String>,
    pub(super) notes: Vec<String>,
}

/// Why the test list could not be read; `line` is 1-based.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct ParseError {
    pub(super) line: usize,
    pub(super) message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "debug_panel.txt line {}: {}",
            self.line, self.message
        )
    }
}

/// An entry while its block is read; checked and completed by [`Draft::finish`].
struct Draft {
    line: usize,
    id: String,
    area: Option<String>,
    title: Option<String>,
    changes: Vec<String>,
    tests: Vec<String>,
    notes: Vec<String>,
}

impl Draft {
    fn new(id: &str, line: usize) -> Self {
        Self {
            line,
            id: id.to_owned(),
            area: None,
            title: None,
            changes: Vec::new(),
            tests: Vec::new(),
            notes: Vec::new(),
        }
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        fn once<T>(slot: &mut Option<T>, key: &str, value: T) -> Result<(), String> {
            if slot.replace(value).is_some() {
                return Err(format!("{key} given twice"));
            }
            Ok(())
        }
        match key {
            "area" => {
                if !AREAS.contains(&value) {
                    return Err(format!("unknown area {value:?}"));
                }
                once(&mut self.area, key, value.to_owned())
            }
            "title" => once(&mut self.title, key, value.to_owned()),
            "change" => {
                self.changes.push(value.to_owned());
                Ok(())
            }
            "test" => {
                self.tests.push(value.to_owned());
                Ok(())
            }
            "note" => {
                self.notes.push(value.to_owned());
                Ok(())
            }
            _ => Err(format!("unknown key {key:?}")),
        }
    }

    fn finish(self) -> Result<Entry, ParseError> {
        let error = |message: &str| ParseError {
            line: self.line,
            message: format!("[{}] {message}", self.id),
        };
        let area = self.area.ok_or_else(|| error("has no area"))?;
        let title = self.title.ok_or_else(|| error("has no title"))?;
        if self.tests.is_empty() {
            return Err(error("has no test step"));
        }
        Ok(Entry {
            id: self.id,
            area,
            title,
            changes: self.changes,
            tests: self.tests,
            notes: self.notes,
        })
    }
}

/// Read a test list in the format the file header describes.
pub(super) fn parse(text: &str) -> Result<Vec<Entry>, ParseError> {
    let mut entries: Vec<Entry> = Vec::new();
    let mut draft: Option<Draft> = None;
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let fail = |message: String| ParseError { line, message };
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if !trimmed.is_ascii() {
            return Err(fail("text must be ASCII".into()));
        }
        if let Some(id) = trimmed
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            if id.is_empty()
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            {
                return Err(fail(format!(
                    "id {id:?} must be lowercase letters, digits and '-'"
                )));
            }
            if entries.iter().any(|entry| entry.id == id)
                || draft.as_ref().is_some_and(|draft| draft.id == id)
            {
                return Err(fail(format!("id {id:?} is used twice")));
            }
            if let Some(done) = draft.replace(Draft::new(id, line)) {
                entries.push(done.finish()?);
            }
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            return Err(fail(format!(
                "expected \"key: value\" or \"[id]\", found {trimmed:?}"
            )));
        };
        let (key, value) = (key.trim(), value.trim());
        let Some(current) = draft.as_mut() else {
            return Err(fail(format!("{key:?} comes before the first [id]")));
        };
        if value.is_empty() {
            return Err(fail(format!("{key} is empty")));
        }
        if value.len() > LINE_LIMIT {
            return Err(fail(format!(
                "{key} is {} characters, more than {LINE_LIMIT}",
                value.len()
            )));
        }
        current.set(key, value).map_err(fail)?;
    }
    if let Some(done) = draft {
        entries.push(done.finish()?);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn embedded() -> Vec<Entry> {
        parse(EMBEDDED).unwrap_or_else(|error| panic!("{error}"))
    }

    #[test]
    fn embedded_list_parses_with_required_fields() {
        let entries = embedded();
        assert!(!entries.is_empty());
        for entry in &entries {
            assert!(!entry.title.is_empty(), "{}", entry.id);
            assert!(!entry.tests.is_empty(), "{}", entry.id);
            assert!(!entry.changes.is_empty(), "{} has no change line", entry.id);
            assert!(
                entry.changes.len() <= 3,
                "{} has too many change lines",
                entry.id
            );
            assert!(AREAS.contains(&entry.area.as_str()), "{}", entry.id);
        }
    }

    #[test]
    fn embedded_ids_are_unique() {
        let entries = embedded();
        let mut ids = HashSet::new();
        for entry in &entries {
            assert!(ids.insert(entry.id.as_str()), "id {} twice", entry.id);
        }
    }

    #[test]
    fn entries_keep_their_area_and_lists() {
        let entries = parse(
            "[a]\narea: Menus & settings\ntitle: T\ntest: Step\n\
             [b]\narea: Audio\ntitle: U\nchange: C\ntest: Step\nnote: N\n",
        )
        .unwrap();
        assert_eq!(entries[0].area, "Menus & settings");
        assert_eq!(entries[1].area, "Audio");
        assert_eq!(entries[1].changes, ["C"]);
        assert_eq!(entries[1].notes, ["N"]);
    }

    #[test]
    fn broken_lists_name_the_line() {
        let base = "[a]\narea: HUD\ntitle: T\ntest: Step\n";
        let cases = [
            ("[a]\ntitle: T\ntest: S\n", 1, "no area"),
            ("[a]\narea: HUD\ntest: S\n", 1, "no title"),
            ("[a]\narea: HUD\ntitle: T\n", 1, "no test"),
            ("[a]\narea: Sky\n", 2, "unknown area"),
            ("[a]\ncolour: red\n", 2, "unknown key"),
            // Keys the list no longer has.
            ("[a]\npr: 1\n", 2, "unknown key"),
            ("[a]\nissue: 2\n", 2, "unknown key"),
            ("[a]\nstatus: personal\n", 2, "unknown key"),
            ("[a]\ntitle: T\ntitle: U\n", 3, "twice"),
            ("[a]\narea: HUD\narea: Audio\n", 3, "twice"),
            ("title: T\n", 1, "before the first"),
            ("[A b]\n", 1, "lowercase"),
            ("[a]\ntitle: caf\u{e9}\n", 2, "ASCII"),
            ("[a]\ntitle:\n", 2, "empty"),
            ("[a]\njust text\n", 2, "key: value"),
        ];
        for (text, line, message) in cases {
            let error = parse(text).unwrap_err();
            assert_eq!(error.line, line, "{text:?}: {error}");
            assert!(error.message.contains(message), "{text:?}: {error}");
        }
        let doubled = format!("{base}{base}");
        let error = parse(&doubled).unwrap_err();
        assert_eq!(error.line, 5);
        assert!(error.message.contains("used twice"), "{error}");
        let long = format!("[a]\ntitle: {}\n", "x".repeat(LINE_LIMIT + 1));
        assert!(parse(&long).unwrap_err().message.contains("more than"));
        let fits = format!("{base}note: {}\n", "x".repeat(LINE_LIMIT));
        assert!(parse(&fits).is_ok());
    }
}
