//! A map's speakers (`target_speaker`, `g_target.c:282-363`): a sound placed in the world
//! as an entity of its own, so that clients hear it from there — looping from the start
//! or toggled by name, played once per use on itself, everywhere, or on whoever set it
//! off, or timed by the client alone (`wait`, `random`). A speaker with a `soundSet`
//! instead is an ambient soundset clients build for themselves (`EF_PERMANENT`).
//!
//! The speaker keeps its noise by name; the legacy projection ([`Speaker::project`]) is
//! handed the index the server registered it at (`G_SoundIndex`, `G_SoundSetIndex`).
//! Held to `tools/game-oracle/speaker.c` (`game-speaker.txt`).

use sjk_entity::Entity;
use sjk_protocol::EntityState;

/// `ET_SPEAKER`.
pub const ET_SPEAKER: u32 = 9;
/// `EV_GENERAL_SOUND`, `EV_GLOBAL_SOUND`.
pub const EV_GENERAL_SOUND: u32 = 76;
pub const EV_GLOBAL_SOUND: u32 = 77;
/// `EF_PERMANENT`: built by the client, never sent.
pub const EF_PERMANENT: u32 = 0x80;
/// Spawnflags: `looped-on`, `looped-off`, `global`, `activator`.
const LOOPED_ON: u32 = 1;
const LOOPED: u32 = 3;
const GLOBAL: u32 = 4;
const ACTIVATOR: u32 = 8;

/// The wire fields a speaker's projection writes (`msg.cpp`'s entity fields).
mod es {
    pub const POS_BASE: [usize; 3] = [2, 1, 4];
    pub const ORIGIN: [usize; 3] = [11, 12, 13];
    pub const ANGLES: [usize; 3] = [25, 9, 24];
    pub const TYPE: usize = 8;
    pub const FLAGS: usize = 19;
    pub const CLIENT: usize = 32;
    pub const EVENT_PARM: usize = 42;
    pub const LOOP_SOUND: usize = 55;
    pub const TRICKED: usize = 58;
    pub const LOOP_IS_SOUNDSET: usize = 70;
    pub const SOUND_SET: usize = 78;
    pub const FRAME: usize = 83;
}

/// What a speaker plays.
#[derive(Clone, Debug, PartialEq)]
pub enum SpeakerKind {
    /// A sound (`noise`), with its spawnflags and the client's own timing: `s.frame` is
    /// `wait` and `s.clientNum` `random`, both in tenths of a second.
    Noise {
        noise: String,
        spawnflags: u32,
        frame: i32,
        random: i32,
    },
    /// An ambient soundset (`soundSet`), which clients play for themselves.
    SoundSet { name: String },
}

/// A speaker of the map.
#[derive(Clone, Debug, PartialEq)]
pub struct Speaker {
    pub kind: SpeakerKind,
    /// `s.origin`, the stationary trajectory's base.
    pub origin: [f32; 3],
    /// `s.angles`, as the map gave them.
    pub angles: [f32; 3],
    /// `targetname`: the name a use reaches it by; empty for none.
    pub targetname: String,
    /// `s.loopSound` is the noise: the loop plays.
    pub looping: bool,
    /// `s.trickedentindex`: 1 when a use turned the loop off, 0 when one turned it on.
    pub tricked: u32,
}

/// What a use asks for beyond the speaker's own state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeakerUse {
    /// A loop toggled, or a soundset speaker (which no use reaches): nothing more.
    None,
    /// `G_AddEvent(self, event, noise)`: the sound played on the speaker.
    OnSelf { event: u32 },
    /// `G_AddEvent(activator, EV_GENERAL_SOUND, noise)`: on whoever set it off.
    OnActivator,
}

/// Why a map's `target_speaker` does not spawn.
#[derive(Clone, Debug, PartialEq)]
pub enum Refused {
    /// `"target_speaker without a noise key at %s"`: the reference stops the map
    /// (`ERR_DROP`); this server refuses the speaker and says so.
    NoNoise { origin: [f32; 3] },
}

impl Speaker {
    /// `SP_target_speaker`: a soundset speaker for a map entity with a `soundSet` key; else
    /// a sound one, a `*` noise made an activator speaker, looping from the start for
    /// spawnflag 1.
    pub fn spawn(entity: &Entity) -> Result<Self, Refused> {
        let origin = crate::fx_runner::vector(entity, "origin");
        let angles = crate::fx_runner::spawn_angles(entity);
        let targetname = entity.get("targetname").unwrap_or_default().to_owned();
        if let Some(name) = entity.get("soundSet") {
            return Ok(Self {
                kind: SpeakerKind::SoundSet {
                    name: name.to_owned(),
                },
                origin,
                angles,
                targetname,
                looping: false,
                tricked: 0,
            });
        }
        let Some(noise) = entity.get("noise") else {
            return Err(Refused::NoNoise { origin });
        };
        let float = |key: &str| {
            entity
                .get(key)
                .map_or(0.0, |value| crate::text_parse::atof(value.as_bytes()))
        };
        let mut spawnflags = entity
            .get("spawnflags")
            .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()))
            as u32;
        if noise.starts_with('*') {
            spawnflags |= ACTIVATOR;
        }
        // `MAX_QPATH`: `Q_strncpyz` into a buffer of 64.
        let noise: String = noise.chars().take(63).collect();
        let (frame, random) = (
            (float("wait") * 10.0) as i32,
            (float("random") * 10.0) as i32,
        );
        let looping = spawnflags & LOOPED_ON != 0;
        Ok(Self {
            kind: SpeakerKind::Noise {
                noise,
                spawnflags,
                frame,
                random,
            },
            origin,
            angles,
            targetname,
            looping,
            tricked: 0,
        })
    }

    /// The name of what it plays: its noise or its soundset.
    pub fn sound(&self) -> &str {
        match &self.kind {
            SpeakerKind::Noise { noise, .. } => noise,
            SpeakerKind::SoundSet { name } => name,
        }
    }

    /// Whether a use reaches it (`use = Use_Target_Speaker`): every sound speaker does.
    pub fn usable(&self) -> bool {
        matches!(self.kind, SpeakerKind::Noise { .. })
    }

    /// `SVF_BROADCAST`: a global speaker is sent to everyone wherever they are.
    pub fn broadcast(&self) -> bool {
        matches!(self.kind, SpeakerKind::Noise { spawnflags, .. } if spawnflags & GLOBAL != 0)
    }

    /// `Use_Target_Speaker`: a looping speaker toggles its loop; any other plays its sound
    /// on whoever set it off (spawnflag 8), everywhere (4), or on itself.
    pub fn use_speaker(&mut self) -> SpeakerUse {
        let SpeakerKind::Noise { spawnflags, .. } = self.kind else {
            return SpeakerUse::None;
        };
        if spawnflags & LOOPED != 0 {
            self.looping = !self.looping;
            self.tricked = u32::from(!self.looping);
            SpeakerUse::None
        } else if spawnflags & ACTIVATOR != 0 {
            SpeakerUse::OnActivator
        } else if spawnflags & GLOBAL != 0 {
            SpeakerUse::OnSelf {
                event: EV_GLOBAL_SOUND,
            }
        } else {
            SpeakerUse::OnSelf {
                event: EV_GENERAL_SOUND,
            }
        }
    }

    /// The speaker as protocol 26 carries it, `index` being its noise's `CS_SOUNDS` index
    /// or its soundset's `CS_AMBIENT_SET` one. The event is the caller's.
    pub fn project(&self, state: &mut EntityState, index: u16) {
        for axis in 0..3 {
            state.set_raw_field(es::ORIGIN[axis], self.origin[axis].to_bits());
            state.set_raw_field(es::POS_BASE[axis], self.origin[axis].to_bits());
            state.set_raw_field(es::ANGLES[axis], self.angles[axis].to_bits());
        }
        match &self.kind {
            SpeakerKind::SoundSet { .. } => {
                state.set_raw_field(es::SOUND_SET, u32::from(index));
                state.set_raw_field(es::FLAGS, EF_PERMANENT);
            }
            SpeakerKind::Noise { frame, random, .. } => {
                state.set_raw_field(es::TYPE, ET_SPEAKER);
                state.set_raw_field(es::EVENT_PARM, u32::from(index));
                state.set_raw_field(es::FRAME, *frame as u32);
                state.set_raw_field(es::CLIENT, *random as u32);
                state.set_raw_field(
                    es::LOOP_SOUND,
                    if self.looping { u32::from(index) } else { 0 },
                );
                state.set_raw_field(es::LOOP_IS_SOUNDSET, 0);
                state.set_raw_field(es::TRICKED, self.tricked);
            }
        }
    }
}
