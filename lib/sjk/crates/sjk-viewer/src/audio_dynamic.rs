//! MP dynamic level selection: codemp/client/snd_music.cpp:420-605,775-811.
//! Marker-timed transitions are deliberately not synthesized here.

#[derive(Clone, Debug, Default)]
struct Group {
    pairs: Vec<(String, String)>,
    groups: Vec<(String, Group)>,
}

impl Group {
    fn group(&self, key: &str) -> Option<&Self> {
        self.groups
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, g)| g)
    }
    fn value(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
            .filter(|v| *v != "placeholder")
    }
}

/// Parsed keyed DMS groups, retaining entry/exit data for future timed transitions.
pub(super) struct Catalogue(Group);

/// Resolved base tracks; requests currently switch immediately at track start.
pub(super) struct Level {
    /// Explore, action, silence, boss and death paths respectively.
    pub tracks: [Option<String>; 5],
    /// Current immediate base-track selection, not a timed transition state.
    pub state: usize,
}

impl Catalogue {
    /// Parse balanced keyed groups without borrowing retail file storage.
    pub fn parse(text: &str) -> Result<Self, String> {
        // GenericParser2 braces, quoted tokens and C-style comments.
        let mut tokens = Vec::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c.is_whitespace() {
                continue;
            }
            if c == '/' && chars.peek() == Some(&'/') {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
                continue;
            }
            if c == '/' && chars.peek() == Some(&'*') {
                chars.next();
                let mut previous = ' ';
                let mut closed = false;
                for c in chars.by_ref() {
                    if previous == '*' && c == '/' {
                        closed = true;
                        break;
                    }
                    previous = c;
                }
                if !closed {
                    return Err("Unterminated DMS comment".into());
                }
                continue;
            }
            if c == '{' || c == '}' {
                tokens.push(c.to_string());
                continue;
            }
            let mut token = String::new();
            if c == '"' {
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '"' {
                        closed = true;
                        break;
                    }
                    token.push(c);
                }
                if !closed {
                    return Err("Unterminated DMS quote".into());
                }
            } else {
                token.push(c);
                while chars
                    .peek()
                    .is_some_and(|c| !c.is_whitespace() && *c != '{' && *c != '}')
                {
                    token.push(chars.next().unwrap());
                }
            }
            tokens.push(token);
        }
        fn group(tokens: &[String], cursor: &mut usize, depth: usize) -> Result<Group, String> {
            if depth > 32 {
                return Err("DMS nesting exceeds 32".into());
            }
            let mut result = Group::default();
            while let Some(key) = tokens.get(*cursor) {
                *cursor += 1;
                if key == "}" {
                    return if depth == 0 {
                        Err("Unexpected DMS brace".into())
                    } else {
                        Ok(result)
                    };
                }
                let value = tokens.get(*cursor).ok_or("Missing DMS value")?;
                *cursor += 1;
                if value == "{" {
                    result
                        .groups
                        .push((key.clone(), group(tokens, cursor, depth + 1)?));
                } else if value == "}" {
                    return Err("Missing DMS value".into());
                } else {
                    result.pairs.push((key.clone(), value.clone()));
                }
            }
            if depth != 0 {
                return Err("Unclosed DMS group".into());
            }
            Ok(result)
        }
        let root = group(&tokens, &mut 0, 0)?;
        if root.group("musicfiles").is_none() || root.group("levelmusic").is_none() {
            return Err("DMS needs musicfiles and levelmusic groups".into());
        }
        Ok(Self(root))
    }

    /// Resolve stock uses/useboss aliases and directory-based track names.
    pub fn level(&self, name: &str) -> Result<Level, String> {
        let levels = self.0.group("levelmusic").unwrap();
        let files = self.0.group("musicfiles").unwrap();
        let mut directory = name.rsplit('/').next().unwrap_or(name);
        for _ in 0..10 {
            let level = levels
                .group(directory)
                .ok_or("Dynamic music level not found")?;
            if let Some(next) = level.value("uses") {
                directory = next;
                continue;
            }
            let mut tracks = std::array::from_fn(|_| None);
            for (index, key) in [(0, "explore"), (1, "action"), (3, "boss")] {
                let (group, dir) = if key == "boss" {
                    if let Some(dir) = level.value("useboss") {
                        (levels.group(dir).ok_or("Missing useboss level")?, dir)
                    } else {
                        (level, directory)
                    }
                } else {
                    (level, directory)
                };
                if let Some(file) = group.value(key) {
                    if files.group(file).is_none() {
                        return Err(format!("Missing musicfiles {file}"));
                    }
                    tracks[index] = Some(format!("music/{dir}/{file}.mp3"));
                }
            }
            if tracks[0].is_none() || tracks[1].is_none() {
                return Err("Dynamic music needs explore and action".into());
            }
            tracks[4] = Some("music/death_music.mp3".into());
            return Ok(Level { tracks, state: 0 });
        }
        Err("DMS uses chain exceeds stock's ten-level limit".into())
    }
}

impl Level {
    /// Validate and select a base state; silence has no asset path.
    pub fn request(&mut self, name: &str) -> Result<Option<&str>, String> {
        let index = ["explore", "action", "silence", "boss", "death"]
            .iter()
            .position(|s| s.eq_ignore_ascii_case(name))
            .ok_or("Unknown dynamic music state")?;
        if index != 2 && self.tracks[index].is_none() {
            return Err("Requested dynamic track unavailable".into());
        }
        self.state = index;
        Ok(self.tracks[index].as_deref())
    }
}
