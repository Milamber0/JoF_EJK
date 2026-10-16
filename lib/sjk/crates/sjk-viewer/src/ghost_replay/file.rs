//! The `.ghost` text format: a header (`sjkghost 1`, the map, the start pose) and one
//! line per recorded command with the state the server confirmed after it.

use super::{Frame, Run, Start};

const MAGIC: &str = "sjkghost 1";

/// Longest recording kept: ten minutes of 125 commands a second.
pub(super) const MAX_FRAMES: usize = 75_000;

impl Run {
    /// The file's text.
    pub(super) fn to_text(&self) -> String {
        let mut text = String::with_capacity(64 + self.frames.len() * 96);
        text.push_str(MAGIC);
        text.push('\n');
        text.push_str(&format!("map {}\n", self.map));
        let s = &self.start;
        text.push_str(&format!(
            "start {} {} {} {} {} {} {} {} {} {}\n",
            s.origin[0],
            s.origin[1],
            s.origin[2],
            s.view[0],
            s.view[1],
            s.view[2],
            s.velocity[0],
            s.velocity[1],
            s.velocity[2],
            s.weapon,
        ));
        for f in &self.frames {
            text.push_str(&format!(
                "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}\n",
                f.time,
                f.view[0],
                f.view[1],
                f.view[2],
                f.forward,
                f.right,
                f.up,
                f.buttons,
                f.weapon,
                f.force_selection,
                f.inventory_selection,
                f.generic_command,
                f.origin[0],
                f.origin[1],
                f.origin[2],
                f.velocity[0],
                f.velocity[1],
                f.velocity[2],
                u8::from(f.ground),
                u8::from(f.confirmed),
            ));
        }
        text
    }

    /// Parse a file's text.
    pub(super) fn from_text(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(MAGIC) {
            return Err("not a ghost file (no sjkghost 1 header)".into());
        }
        let map = lines
            .next()
            .and_then(|line| line.strip_prefix("map "))
            .map(|map| map.trim().to_owned())
            .ok_or("missing map line")?;
        let start = lines
            .next()
            .and_then(|line| line.strip_prefix("start "))
            .ok_or("missing start line")?;
        let start = fields(start, 10)?;
        let start = Start {
            origin: [num(start[0])?, num(start[1])?, num(start[2])?],
            view: [num(start[3])?, num(start[4])?, num(start[5])?],
            velocity: [num(start[6])?, num(start[7])?, num(start[8])?],
            weapon: num(start[9])?,
        };
        let mut frames = Vec::new();
        for (index, line) in lines.enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            if frames.len() == MAX_FRAMES {
                return Err(format!("more than {MAX_FRAMES} commands"));
            }
            let f = fields(line, 20).map_err(|error| format!("command {index}: {error}"))?;
            let frame = (|| {
                Ok::<_, String>(Frame {
                    time: num(f[0])?,
                    view: [num(f[1])?, num(f[2])?, num(f[3])?],
                    forward: num(f[4])?,
                    right: num(f[5])?,
                    up: num(f[6])?,
                    buttons: num(f[7])?,
                    weapon: num(f[8])?,
                    force_selection: num(f[9])?,
                    inventory_selection: num(f[10])?,
                    generic_command: num(f[11])?,
                    origin: [num(f[12])?, num(f[13])?, num(f[14])?],
                    velocity: [num(f[15])?, num(f[16])?, num(f[17])?],
                    ground: num::<u8>(f[18])? != 0,
                    confirmed: num::<u8>(f[19])? != 0,
                })
            })()
            .map_err(|error| format!("command {index}: {error}"))?;
            if frames
                .last()
                .is_some_and(|previous: &Frame| frame.time < previous.time)
            {
                return Err(format!("command {index}: time goes backwards"));
            }
            frames.push(frame);
        }
        if frames.is_empty() {
            return Err("no commands recorded".into());
        }
        Ok(Self { map, start, frames })
    }
}

fn fields(line: &str, count: usize) -> Result<Vec<&str>, String> {
    let fields: Vec<&str> = line.split_ascii_whitespace().collect();
    if fields.len() == count {
        Ok(fields)
    } else {
        Err(format!("expected {count} fields, found {}", fields.len()))
    }
}

fn num<T: std::str::FromStr>(text: &str) -> Result<T, String> {
    text.parse().map_err(|_| format!("bad number {text:?}"))
}

/// A ghost's file name from what the player typed: letters, digits, `-` and `_`.
pub(super) fn file_name(name: &str) -> Result<String, String> {
    let name = name.strip_suffix(".ghost").unwrap_or(name);
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("a ghost name uses letters, digits, - and _ (64 at most)".into());
    }
    Ok(format!("{name}.ghost"))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn run() -> Run {
        Run {
            map: "mp/ffa3".into(),
            start: Start {
                origin: [1.5, -2.25, 24.0],
                view: [100, -16384, 0],
                velocity: [0.0; 3],
                weapon: 3,
            },
            frames: vec![
                Frame {
                    time: 0,
                    view: [100, -16384, 0],
                    forward: 127,
                    right: -127,
                    up: 0,
                    buttons: 1,
                    weapon: 3,
                    force_selection: 1,
                    inventory_selection: 255,
                    generic_command: 0,
                    origin: [2.0, -2.25, 24.0],
                    velocity: [62.5, 0.0, 0.0],
                    ground: true,
                    confirmed: true,
                },
                Frame {
                    time: 8,
                    up: 127,
                    generic_command: 12,
                    ground: false,
                    confirmed: false,
                    ..run_frame()
                },
            ],
        }
    }

    fn run_frame() -> Frame {
        Frame {
            time: 0,
            view: [0; 3],
            forward: 0,
            right: 0,
            up: 0,
            buttons: 0,
            weapon: 3,
            force_selection: 0,
            inventory_selection: 255,
            generic_command: 0,
            origin: [0.1, 0.2, 0.3],
            velocity: [0.0; 3],
            ground: true,
            confirmed: false,
        }
    }

    #[test]
    fn a_run_survives_its_text() {
        let run = run();
        let back = Run::from_text(&run.to_text()).expect("parse");
        assert_eq!(back.map, run.map);
        assert_eq!(back.start, run.start);
        assert_eq!(back.frames, run.frames);
    }

    #[test]
    fn broken_files_are_refused_with_a_reason() {
        assert!(Run::from_text("hello").is_err());
        let text = run().to_text();
        let truncated: String = text.lines().take(3).collect::<Vec<_>>().join("\n");
        assert_eq!(
            Run::from_text(&truncated).err().as_deref(),
            Some("no commands recorded")
        );
        let short = format!("{}\n1 2 3\n", truncated);
        assert!(
            Run::from_text(&short)
                .unwrap_err()
                .contains("expected 20 fields")
        );
        let mut backwards = run();
        backwards.frames[1].time = -8;
        assert!(
            Run::from_text(&backwards.to_text())
                .unwrap_err()
                .contains("backwards")
        );
    }

    #[test]
    fn names_are_plain() {
        assert_eq!(file_name("pillars").as_deref(), Ok("pillars.ghost"));
        assert_eq!(file_name("pillars.ghost").as_deref(), Ok("pillars.ghost"));
        for bad in ["", "../x", "a b", "c:\\x", "é"] {
            assert!(file_name(bad).is_err(), "{bad}");
        }
    }
}
