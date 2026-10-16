//! Parser for Raven's ambient-set catalog (`sound/sound.txt`).
//!
//! The grammar mirrors `AS_ParseHeader`/`AS_ParseSet` and the keyword readers
//! in `codemp/client/snd_ambient.cpp:274-665`: `generalSet`, `localSet`, and
//! `bmodelSet` records with `timeBetweenWaves`, `subWaves`, `loopedWave`,
//! `volRange`, and `radius`. Only BMODEL stage lookup is consumed by the MP
//! adapter; general/local timed sub-waves remain owned by the ambient-world
//! feature rather than being guessed here.

use std::collections::BTreeMap;

/// Ambient record family declared by `sound/sound.txt`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AmbientSetKind {
    General,
    Local,
    Bmodel,
}

/// One parsed ambient record. Paths are normalized to Raven's `sound/...wav`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmbientSet {
    /// Case-preserving set name following the record keyword.
    pub name: Box<str>,
    /// Source record family (`generalSet`, `localSet`, or `bmodelSet`).
    pub kind: AmbientSetKind,
    /// Minimum/maximum random delay in the source catalog's integer units.
    pub time_between_waves: [i32; 2],
    /// Ordered sound paths; BMODEL uses indices start=0, mid=1, end=2.
    pub sub_waves: Vec<Box<str>>,
    /// Optional continuously looping ambience path.
    pub looped_wave: Option<Box<str>>,
    /// Source-authored minimum/maximum volume bytes.
    pub volume_range: [i32; 2],
    /// Source-authored audible radius in world units.
    pub radius: i32,
}

impl AmbientSet {
    fn new(name: &str, kind: AmbientSetKind) -> Self {
        Self {
            name: name.into(),
            kind,
            time_between_waves: [10, 25],
            sub_waves: Vec::new(),
            looped_wave: None,
            volume_range: [255, 255],
            radius: 250,
        }
    }
}

/// Case-insensitive catalog parsed once at gamestate installation.
#[derive(Clone, Debug, Default)]
pub struct AmbientSets {
    sets: BTreeMap<String, AmbientSet>,
    declarations: [usize; 3],
}

impl AmbientSets {
    /// Parse the line-oriented format accepted by Raven's ambient backend.
    pub fn parse(text: &str) -> Self {
        let lines = text.lines().collect::<Vec<_>>();
        let mut result = Self::default();
        let mut index = 0;
        while index < lines.len() {
            let header = tokens(lines[index]);
            let kind = header
                .first()
                .and_then(|token| match token.to_ascii_lowercase().as_str() {
                    "generalset" => Some(AmbientSetKind::General),
                    "localset" => Some(AmbientSetKind::Local),
                    "bmodelset" => Some(AmbientSetKind::Bmodel),
                    _ => None,
                });
            let Some(kind) = kind else {
                index += 1;
                continue;
            };
            let Some(name) = header.get(1) else {
                index += 1;
                continue;
            };
            result.declarations[kind_index(kind)] += 1;
            let mut set = AmbientSet::new(name, kind);
            index += 1;
            while index < lines.len() {
                let fields = tokens(lines[index]);
                if fields.first().is_some_and(|field| is_set_header(field)) {
                    break;
                }
                if let Some(keyword) = fields.first().map(|field| field.to_ascii_lowercase()) {
                    match keyword.as_str() {
                        "timebetweenwaves" if fields.len() >= 3 => {
                            set.time_between_waves =
                                sorted_pair(parse_i32(fields[1]), parse_i32(fields[2]));
                        }
                        "subwaves" if fields.len() >= 3 => {
                            let directory = fields[1];
                            set.sub_waves.extend(fields[2..].iter().map(|wave| {
                                format!("sound/{directory}/{wave}.wav").into_boxed_str()
                            }));
                        }
                        "loopedwave" if fields.len() >= 2 => {
                            set.looped_wave =
                                Some(format!("sound/{}.wav", fields[1]).into_boxed_str());
                        }
                        "volrange" if fields.len() >= 3 => {
                            set.volume_range =
                                sorted_pair(parse_i32(fields[1]), parse_i32(fields[2]));
                        }
                        "radius" if fields.len() >= 2 => set.radius = parse_i32(fields[1]),
                        _ => {}
                    }
                }
                index += 1;
            }
            result.sets.insert(name.to_ascii_lowercase(), set);
        }
        result
    }

    /// Case-insensitive lookup matching `CSetGroup::GetSet`.
    pub fn get(&self, name: &str) -> Option<&AmbientSet> {
        self.sets.get(&name.to_ascii_lowercase())
    }

    /// Number of records in one source grammar family.
    pub fn count_kind(&self, kind: AmbientSetKind) -> usize {
        self.sets.values().filter(|set| set.kind == kind).count()
    }

    /// Number of source declarations, including later duplicate names.
    pub fn declaration_count(&self, kind: AmbientSetKind) -> usize {
        self.declarations[kind_index(kind)]
    }
}

const fn kind_index(kind: AmbientSetKind) -> usize {
    match kind {
        AmbientSetKind::General => 0,
        AmbientSetKind::Local => 1,
        AmbientSetKind::Bmodel => 2,
    }
}

fn tokens(line: &str) -> Vec<&str> {
    line.split_once(';')
        .map_or(line, |(prefix, _)| prefix)
        .split_ascii_whitespace()
        .collect()
}

fn is_set_header(token: &str) -> bool {
    matches!(
        token.to_ascii_lowercase().as_str(),
        "generalset" | "localset" | "bmodelset"
    )
}

fn parse_i32(value: &str) -> i32 {
    value.parse().unwrap_or(0)
}

fn sorted_pair(left: i32, right: i32) -> [i32; 2] {
    if left <= right {
        [left, right]
    } else {
        [right, left]
    }
}
