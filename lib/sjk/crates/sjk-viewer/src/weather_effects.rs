//! The world effect commands: what a map's `*rain`, `*fog`, `*constantwind ( x y z )`
//! effect names and the `r_we` console command ask for (`RE_WorldEffectCommand`,
//! rd-vanilla `tr_WorldEffects.cpp:1463-1852`).
//!
//! A command adds a particle cloud, a wind zone or a weather zone, or clears them. The
//! clouds keep the reference's parameters (count, size, gravity, colour, mass, spawn
//! range); how they are drawn is SJK's own (`weather.wgsl`). The reference's limits
//! hold: five clouds, ten wind zones, ten weather zones; a command past a limit is
//! ignored, as there.

use super::wind::WindZone;

/// `MAX_PARTICLE_CLOUDS`.
pub(crate) const MAX_CLOUDS: usize = 5;
/// `MAX_WIND_ZONES`.
pub(crate) const MAX_WIND_ZONES: usize = 10;
/// `MAX_WEATHER_ZONES`.
pub(crate) const MAX_WEATHER_ZONES: usize = 10;

/// What a cloud's particles look like.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Look {
    /// A rain streak along the particle's velocity (`gfx/world/rain.jpg` triangles in
    /// the reference, drawn procedurally here).
    Streak,
    /// A camera-facing sprite of one of the weather images.
    Sprite(Image),
}

/// The images the sprite clouds use, in texture-array layer order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Image {
    /// `gfx/effects/snowflake1`.
    Snowflake = 0,
    /// `gfx/effects/snowpuff1`.
    SnowPuff = 1,
    /// `gfx/effects/alpha_smoke2b`.
    Smoke = 2,
}

impl Image {
    /// Every image with its path, in layer order.
    pub(crate) const ALL: [(Self, &'static str); 3] = [
        (Self::Snowflake, "gfx/effects/snowflake1"),
        (Self::SnowPuff, "gfx/effects/snowpuff1"),
        (Self::Smoke, "gfx/effects/alpha_smoke2b"),
    ];
}

/// One particle cloud (`CWeatherParticleCloud`) as its command configured it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Cloud {
    /// `mParticleCount`.
    pub(crate) count: u32,
    pub(crate) look: Look,
    /// `mWidth`, `mHeight`.
    pub(crate) width: f32,
    pub(crate) height: f32,
    /// `mGravity`, units per second per unit of mass.
    pub(crate) gravity: f32,
    /// `mColor`, display values; alpha is the fade-in target.
    pub(crate) color: [f32; 4],
    /// `mBlendMode 1`: added to the scene; otherwise alpha-blended.
    pub(crate) additive: bool,
    /// `mMass`: the range each particle's mass is picked from.
    pub(crate) mass: [f32; 2],
    /// `mSpawnRange`: the box around the camera the particles live in.
    pub(crate) range: [[f32; 3]; 2],
    /// `mRotationChangeNext 0`: the sprites turn slowly.
    pub(crate) rotates: bool,
    /// `mWaterParticles`: rain or snow, which a saber hisses in.
    pub(crate) water: bool,
    /// SJK: the haze the falling weather leaves in open air, extinction per unit
    /// (the volumetric fog, `r_weatherQuality` 1 and up).
    pub(crate) haze: f32,
    /// SJK: the ground fog a fog command stands for, drawn as volumetric fog instead
    /// of its sprites from `r_weatherQuality` 1.
    pub(crate) mist: Option<Mist>,
}

/// Fog lying on the ground: densest at the floor, thinning upwards.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Mist {
    /// Extinction per unit at the floor.
    pub(crate) density: f32,
    /// Units over which it thins to about a third.
    pub(crate) height: f32,
}

impl Mist {
    /// `r_weatherFog 2` on a map whose weather has no fog of its own.
    pub(crate) const DEFAULT: Self = Self {
        density: 6.0e-4,
        height: 130.0,
    };
}

impl Cloud {
    /// `CWeatherParticleCloud::Reset`'s defaults for `count` particles.
    fn new(count: u32, look: Look) -> Self {
        let spawn = 500.0 * 1.25;
        Self {
            count,
            look,
            width: 1.0,
            height: 1.0,
            gravity: 300.0,
            color: [1.0; 4],
            additive: false,
            mass: [5.0, 10.0],
            range: [[-spawn; 3], [spawn; 3]],
            rotates: false,
            water: false,
            haze: 0.0,
            mist: None,
        }
    }

    /// The rain the four rain commands make: `mHeight 80`, `mFilterMode 1`,
    /// `mBlendMode 1`, oriented with its velocity. `haze` is SJK's.
    fn rain(count: u32, width: f32, gravity: f32, color: [f32; 4], haze: f32) -> Self {
        Self {
            width,
            height: 80.0,
            gravity,
            color,
            additive: true,
            water: true,
            haze,
            ..Self::new(count, Look::Streak)
        }
    }

    /// The blowing smoke the fog commands make (gravity 0, additive, turning), and
    /// the ground fog SJK draws for it.
    fn mist(count: u32, size: f32, color: [f32; 4], mass: [f32; 2], mist: Mist) -> Self {
        let mut cloud = Self {
            width: size,
            height: size,
            gravity: 0.0,
            color,
            additive: true,
            mass,
            rotates: true,
            mist: Some(mist),
            ..Self::new(count, Look::Sprite(Image::Smoke))
        };
        cloud.range[0][2] = -150.0;
        cloud.range[1][2] = 150.0;
        cloud
    }

    /// A streak of rain rather than a sprite.
    pub(crate) fn is_rain(&self) -> bool {
        self.look == Look::Streak
    }
}

/// Everything the world effect commands have set up.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Effects {
    pub(crate) clouds: Vec<Cloud>,
    pub(crate) winds: Vec<WindZone>,
    /// `zone ( mins ) ( maxs )`: weather zones added by command, beside the map's
    /// `misc_weather_zone` brushes.
    pub(crate) zones: Vec<[[f32; 3]; 2]>,
    /// `freeze`: particles stop moving.
    pub(crate) frozen: bool,
    /// `outsideshake` and `outsidepain`: recorded, not acted on (no camera shake or
    /// damage hint in SJK).
    pub(crate) outside_shake: bool,
    pub(crate) outside_pain: f32,
}

/// The `r_we` help the reference prints for an unknown command.
pub(crate) const HELP: &str = "Weather Effect: Please enter a valid command: die, clear, \
    freeze, zone (mins) (maxs), wind, constantwind (velocity), gustingwind, lightrain, rain, \
    acidrain, heavyrain, snow, spacedust <count>, sand, fog, heavyrainfog, light_fog, \
    outsideshake, outsidepain";

impl Effects {
    /// Apply the commands in order, from nothing: what a map's effect names amount to.
    pub(crate) fn from_commands<'a>(commands: impl IntoIterator<Item = &'a str>) -> Self {
        let mut effects = Self::default();
        for command in commands {
            // A map's name the reference does not know only prints its help there.
            let _ = effects.apply(command);
        }
        effects
    }

    /// Nothing to draw and no wind.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.clouds.is_empty() && self.winds.is_empty()
    }

    /// Run one command (`RE_WorldEffectCommand`). An unknown one is an error carrying
    /// the reference's help.
    pub(crate) fn apply(&mut self, command: &str) -> Result<(), &'static str> {
        let mut tokens = command.split_whitespace();
        let Some(token) = tokens.next() else {
            return Ok(());
        };
        let token = token.to_ascii_lowercase();
        let cloud = match token.as_str() {
            "die" | "clear" => {
                // `die` also forgets the weather zones (`R_ShutdownWorldEffects`).
                if token == "die" {
                    self.zones.clear();
                    self.frozen = false;
                    self.outside_shake = false;
                    self.outside_pain = 0.0;
                }
                self.clouds.clear();
                self.winds.clear();
                return Ok(());
            }
            "freeze" => {
                self.frozen = !self.frozen;
                return Ok(());
            }
            "zone" => {
                if let (Some(mins), Some(maxs)) = (vector(&mut tokens), vector(&mut tokens))
                    && self.zones.len() < MAX_WEATHER_ZONES
                {
                    self.zones.push([mins, maxs]);
                }
                return Ok(());
            }
            "wind" | "constantwind" | "gustingwind" => {
                if self.winds.len() < MAX_WIND_ZONES {
                    self.winds.push(match token.as_str() {
                        "wind" => WindZone::basic(),
                        // A malformed vector blows 800 along +Y, as the reference's does.
                        "constantwind" => {
                            WindZone::constant(vector(&mut tokens).unwrap_or([0.0, 800.0, 0.0]))
                        }
                        _ => WindZone::gusting(),
                    });
                }
                return Ok(());
            }
            "outsideshake" => {
                self.outside_shake = !self.outside_shake;
                return Ok(());
            }
            "outsidepain" => {
                self.outside_pain = if self.outside_pain != 0.0 { 0.0 } else { 1.0 };
                return Ok(());
            }
            // Haze: about half the light lost over 8000 units in drizzle, 2500 in a storm.
            "lightrain" => Cloud::rain(500, 1.2, 2000.0, [0.5; 4], 0.9e-4),
            "rain" => Cloud::rain(1000, 1.2, 2000.0, [0.5; 4], 1.6e-4),
            "acidrain" => {
                self.outside_pain = 0.1;
                Cloud::rain(1000, 2.0, 2000.0, [0.34, 0.70, 0.34, 0.70], 1.6e-4)
            }
            "heavyrain" => Cloud::rain(1000, 1.2, 2800.0, [0.5; 4], 2.8e-4),
            "snow" => Cloud {
                additive: true,
                rotates: true,
                color: [0.75; 4],
                water: true,
                haze: 2.0e-4,
                ..Cloud::new(1000, Look::Sprite(Image::Snowflake))
            },
            "spacedust" => {
                // `atoi` of the next token: a missing count is none.
                let count = tokens.next().map_or(0, |count| {
                    crate::weather::atoi(count).clamp(0, i32::MAX as i64) as u32
                });
                Cloud {
                    width: 1.2,
                    height: 1.2,
                    gravity: 0.0,
                    additive: true,
                    rotates: true,
                    color: [0.75; 4],
                    water: true,
                    mass: [10.0, 30.0],
                    range: [[-1500.0; 3], [1500.0; 3]],
                    ..Cloud::new(count, Look::Sprite(Image::SnowPuff))
                }
            }
            // A sand storm keeps its sprites: its haze is the dust between them.
            "sand" => Cloud {
                additive: false,
                color: [0.9, 0.6, 0.0, 0.5],
                haze: 1.5e-4,
                mist: None,
                ..Cloud::mist(400, 70.0, [0.0; 4], [10.0, 30.0], Mist::DEFAULT)
            },
            "fog" => Cloud::mist(
                60,
                70.0,
                [0.2; 4],
                [10.0, 30.0],
                Mist {
                    density: 9.0e-4,
                    height: 140.0,
                },
            ),
            "heavyrainfog" => Cloud::mist(
                70,
                100.0,
                [0.3; 4],
                [5.0, 10.0],
                Mist {
                    density: 1.4e-3,
                    height: 180.0,
                },
            ),
            "light_fog" => Cloud::mist(
                40,
                100.0,
                [0.19, 0.6, 0.7, 0.12],
                [10.0, 30.0],
                Mist {
                    density: 5.0e-4,
                    height: 110.0,
                },
            ),
            _ => return Err(HELP),
        };
        if self.clouds.len() < MAX_CLOUDS {
            self.clouds.push(cloud);
        }
        Ok(())
    }
}

/// `WE_ParseVector`: `( x y z )`, the parentheses separate tokens.
fn vector<'a>(tokens: &mut impl Iterator<Item = &'a str>) -> Option<[f32; 3]> {
    if tokens.next()? != "(" {
        return None;
    }
    let mut value = [0.0; 3];
    for component in &mut value {
        *component = crate::weather::atof(tokens.next()?);
    }
    (tokens.next()? == ")").then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rain_commands_keep_the_reference_parameters() {
        let effects = Effects::from_commands(["heavyrain", "heavyrainfog"]);
        let [rain, fog] = effects.clouds.as_slice() else {
            panic!("two clouds")
        };
        assert_eq!(
            (rain.count, rain.look, rain.height),
            (1000, Look::Streak, 80.0)
        );
        assert_eq!(
            (rain.gravity, rain.width, rain.additive),
            (2800.0, 1.2, true)
        );
        assert!(rain.water && rain.is_rain());
        assert_eq!((fog.count, fog.look), (70, Look::Sprite(Image::Smoke)));
        assert_eq!((fog.width, fog.gravity, fog.color[0]), (100.0, 0.0, 0.3));
        assert_eq!(fog.range, [[-625.0, -625.0, -150.0], [625.0, 625.0, 150.0]]);
        assert_eq!(fog.mass, [5.0, 10.0]);
    }

    #[test]
    fn a_constant_wind_reads_its_vector_and_falls_back_to_the_reference_default() {
        let effects = Effects::from_commands([
            "constantwind ( -5000.000000 0.000000 0.000000 )",
            "constantwind (1 2 3)",
        ]);
        assert_eq!(effects.winds[0].current(), [-5000.0, 0.0, 0.0]);
        assert_eq!(effects.winds[1].current(), [0.0, 800.0, 0.0]);
    }

    #[test]
    fn limits_clear_die_and_unknown_commands_follow_the_reference() {
        let mut effects = Effects::from_commands(["rain"; 7]);
        assert_eq!(effects.clouds.len(), MAX_CLOUDS);
        assert_eq!(effects.apply("ZONE ( 0 0 0 ) ( 64 64 64 )"), Ok(()));
        assert_eq!(effects.apply("wind"), Ok(()));
        assert_eq!(effects.apply("clear"), Ok(()));
        assert!(effects.is_empty());
        assert_eq!(effects.zones, [[[0.0; 3], [64.0; 3]]]);
        assert_eq!(effects.apply("die"), Ok(()));
        assert!(effects.zones.is_empty());
        assert_eq!(effects.apply("drizzle"), Err(HELP));
        assert_eq!(effects.apply("freeze"), Ok(()));
        assert!(effects.frozen);
    }

    #[test]
    fn space_dust_counts_and_acid_rain_hurts_outside() {
        let effects = Effects::from_commands(["spacedust 300", "acidrain", "spacedust"]);
        assert_eq!(effects.clouds[0].count, 300);
        assert_eq!(effects.clouds[0].range, [[-1500.0; 3], [1500.0; 3]]);
        assert_eq!(effects.clouds[1].color, [0.34, 0.70, 0.34, 0.70]);
        assert_eq!(effects.outside_pain, 0.1);
        assert_eq!(effects.clouds[2].count, 0);
    }
}
