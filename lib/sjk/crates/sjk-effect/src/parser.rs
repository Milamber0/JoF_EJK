//! Token parser for the generic Raven EFX grammar.

use crate::{
    Component, ComponentKind, Curve, CurveFlags, CurveModifier, EffectDefinition, EffectError,
    Range, VectorRange,
};

pub(crate) fn parse(source: &str) -> Result<EffectDefinition, EffectError> {
    let tokens = tokenize(source);
    let mut cursor = Cursor::new(&tokens);
    let mut effect = EffectDefinition::default();
    while let Some(token) = cursor.next() {
        if token.eq_ignore_ascii_case("repeatdelay") {
            effect.repeat_delay = Some(parse_range(&mut cursor)?);
        } else if let Some(kind) = component_kind(token) {
            cursor.expect("{")?;
            effect.components.push(parse_component(&mut cursor, kind)?);
        } else {
            skip_value(&mut cursor);
        }
    }
    Ok(effect)
}

fn parse_component(cursor: &mut Cursor<'_>, kind: ComponentKind) -> Result<Component, EffectError> {
    let mut component = Component::new(kind);
    while let Some(key) = cursor.next() {
        if key == "}" {
            return Ok(component);
        }
        match key.to_ascii_lowercase().as_str() {
            "count" => component.count = parse_range(cursor)?,
            "life" => component.life = parse_range(cursor)?,
            "delay" => component.delay = parse_range(cursor)?,
            "origin" => component.origin = parse_vector_range(cursor)?,
            "origin2" => component.origin2 = parse_vector_range(cursor)?,
            "velocity" | "vel" => component.velocity = parse_vector_range(cursor)?,
            "acceleration" | "accel" => component.acceleration = parse_vector_range(cursor)?,
            "gravity" => component.gravity = parse_range(cursor)?,
            "rotation" => component.rotation = parse_range(cursor)?,
            "rotationdelta" => component.rotation_delta = parse_range(cursor)?,
            "angle" | "angles" => component.angles = parse_vector_range(cursor)?,
            "angledelta" => component.angle_delta = parse_vector_range(cursor)?,
            "density" => component.density = parse_range(cursor)?,
            "variance" => component.variance = parse_range(cursor)?,
            "cullrange" => component.cull_range = Some(parse_range(cursor)?),
            "size" | "width" => component.size = parse_curve(cursor, component.size)?,
            "size2" | "width2" => {
                component.size2 = parse_curve(cursor, component.size2)?;
                component.size2_authored = true;
            }
            "length" => {
                component.length = parse_curve(cursor, component.length)?;
                component.length_authored = true;
            }
            "alpha" => component.alpha = parse_curve(cursor, component.alpha)?,
            "rgb" => parse_rgb(cursor, &mut component)?,
            // Retail reads both keys into the one `mElasticity` and sets `FX_APPLY_PHYSICS`
            // (`FxTemplate.cpp:2128-2129`, `ParseElasticity` `:448-458`). That value is a
            // particle's bounce, an electricity bolt's jaggedness (`FxScheduler.cpp:1502-1508`)
            // and a camera shake's intensity. `elasticity` and `chaos` are not retail keys.
            "bounce" | "intensity" => {
                let elasticity = parse_range(cursor)?;
                component.elasticity = elasticity;
                component.chaos = elasticity;
                component.chaos_authored = true;
                component.intensity = elasticity;
                component.flags.apply_physics = true;
                if key.eq_ignore_ascii_case("bounce") {
                    component.bounce_authored = true;
                } else {
                    component.intensity_authored = true;
                }
            }
            "radius" => component.radius = parse_range(cursor)?,
            "height" if cursor.peek() == Some("{") => {
                component.length = parse_curve(cursor, component.length)?;
                component.length_authored = true;
            }
            "height" => component.height = parse_range(cursor)?,
            "shader" | "shaders" => component.shaders = parse_list(cursor)?,
            "model" | "models" => {
                component.models = parse_list(cursor)?;
                component.flags.use_model = true;
            }
            "playfx" => component.effects = parse_list(cursor)?,
            "emitfx" => {
                component.emit_effects = parse_list(cursor)?;
                component.flags.emit_effect = true;
            }
            "impactfx" => component.impact_effects = parse_list(cursor)?,
            "deathfx" => component.death_effects = parse_list(cursor)?,
            "sounds" => component.sounds = parse_list(cursor)?,
            "flags" => {
                let mut flags = crate::parser_flags::primitive(cursor);
                flags.apply_physics |= component.flags.apply_physics;
                component.flags = flags;
            }
            "spawnflags" => component.spawn_flags = crate::parser_flags::spawn(cursor),
            "name" => skip_name(cursor),
            _ => skip_value(cursor),
        }
    }
    Err(EffectError::Parse("unterminated effect component".into()))
}

fn parse_curve(cursor: &mut Cursor<'_>, mut curve: Curve) -> Result<Curve, EffectError> {
    cursor.expect("{")?;
    while let Some(key) = cursor.next() {
        if key == "}" {
            return Ok(curve);
        }
        match key.to_ascii_lowercase().as_str() {
            "start" => curve.start = parse_range(cursor)?,
            "end" => curve.end = parse_range(cursor)?,
            "parm" | "parms" => curve.parameter = parse_range(cursor)?,
            "flags" => curve.flags = parse_curve_flags(cursor),
            _ => skip_value(cursor),
        }
    }
    Err(EffectError::Parse("unterminated effect curve".into()))
}

fn parse_rgb(cursor: &mut Cursor<'_>, component: &mut Component) -> Result<(), EffectError> {
    cursor.expect("{")?;
    while let Some(key) = cursor.next() {
        if key == "}" {
            return Ok(());
        }
        match key.to_ascii_lowercase().as_str() {
            "start" => component.rgb_start = parse_rgb_ranges(cursor)?,
            "end" => component.rgb_end = parse_rgb_ranges(cursor)?,
            "parm" | "parms" => component.rgb_parameter = parse_range(cursor)?,
            "flags" => component.rgb_flags = parse_curve_flags(cursor),
            _ => skip_value(cursor),
        }
    }
    Err(EffectError::Parse("unterminated rgb curve".into()))
}

fn parse_range(cursor: &mut Cursor<'_>) -> Result<Range, EffectError> {
    let minimum = cursor.number()?;
    let maximum = cursor
        .peek()
        .and_then(|value| value.parse::<f32>().ok())
        .map_or(minimum, |_| cursor.number().unwrap_or(minimum));
    Ok(Range { minimum, maximum })
}

fn parse_vector_range(cursor: &mut Cursor<'_>) -> Result<VectorRange, EffectError> {
    let values = parse_numbers(cursor, 6)?;
    match values.len() {
        3 => Ok(VectorRange {
            minimum: [values[0], values[1], values[2]],
            maximum: [values[0], values[1], values[2]],
        }),
        6 => Ok(VectorRange {
            minimum: [values[0], values[1], values[2]],
            maximum: [values[3], values[4], values[5]],
        }),
        _ => Err(EffectError::Parse(
            "vector property requires 3 or 6 numbers".into(),
        )),
    }
}

fn parse_rgb_ranges(cursor: &mut Cursor<'_>) -> Result<[Range; 3], EffectError> {
    let values = parse_numbers(cursor, 6)?;
    match values.len() {
        3 => Ok(std::array::from_fn(|index| Range {
            minimum: values[index],
            maximum: values[index],
        })),
        6 => Ok(std::array::from_fn(|index| Range {
            minimum: values[index],
            maximum: values[index + 3],
        })),
        _ => Err(EffectError::Parse(
            "rgb property requires 3 or 6 numbers".into(),
        )),
    }
}

fn parse_numbers(cursor: &mut Cursor<'_>, limit: usize) -> Result<Vec<f32>, EffectError> {
    let mut values = Vec::new();
    while values.len() < limit {
        let Some(value) = cursor.peek().and_then(|token| token.parse::<f32>().ok()) else {
            break;
        };
        cursor.next();
        values.push(value);
    }
    if values.is_empty() {
        Err(EffectError::Parse("expected a number".into()))
    } else {
        Ok(values)
    }
}

fn parse_list(cursor: &mut Cursor<'_>) -> Result<Vec<String>, EffectError> {
    cursor.expect("[")?;
    let mut values = Vec::new();
    while let Some(value) = cursor.next() {
        if value == "]" {
            return Ok(values);
        }
        values.push(value.to_owned());
    }
    Err(EffectError::Parse("unterminated effect value list".into()))
}

fn parse_curve_flags(cursor: &mut Cursor<'_>) -> CurveFlags {
    let mut flags = CurveFlags::default();
    while let Some(token) = cursor.peek() {
        match token.to_ascii_lowercase().as_str() {
            "linear" => flags.linear = true,
            "random" => flags.random = true,
            "nonlinear" => flags.modifier = CurveModifier::NonLinear,
            "wave" => flags.modifier = CurveModifier::Wave,
            "clamp" => flags.modifier = CurveModifier::Clamp,
            _ => break,
        }
        cursor.next();
    }
    flags
}

fn component_kind(token: &str) -> Option<ComponentKind> {
    Some(match token.to_ascii_lowercase().as_str() {
        "particle" => ComponentKind::Particle,
        "orientedparticle" => ComponentKind::OrientedParticle,
        "line" => ComponentKind::Line,
        "tail" => ComponentKind::Tail,
        "cylinder" => ComponentKind::Cylinder,
        "electricity" => ComponentKind::Electricity,
        "fxrunner" => ComponentKind::FxRunner,
        "decal" => ComponentKind::Decal,
        "sound" => ComponentKind::Sound,
        "light" => ComponentKind::Light,
        "camerashake" => ComponentKind::CameraShake,
        "flash" => ComponentKind::Flash,
        "emitter" => ComponentKind::Emitter,
        _ if token
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_uppercase()) =>
        {
            ComponentKind::Other
        }
        _ => return None,
    })
}

fn skip_name(cursor: &mut Cursor<'_>) {
    while cursor
        .peek()
        .is_some_and(|token| token != "}" && !is_property(token.to_ascii_lowercase().as_str()))
    {
        cursor.next();
    }
}

fn is_property(token: &str) -> bool {
    matches!(
        token,
        "count"
            | "life"
            | "delay"
            | "origin"
            | "origin2"
            | "velocity"
            | "vel"
            | "acceleration"
            | "accel"
            | "gravity"
            | "rotation"
            | "rotationdelta"
            | "cullrange"
            | "size"
            | "width"
            | "size2"
            | "width2"
            | "length"
            | "alpha"
            | "rgb"
            | "intensity"
            | "angle"
            | "angles"
            | "angledelta"
            | "density"
            | "variance"
            | "model"
            | "models"
            | "emitfx"
            | "bounce"
            | "radius"
            | "height"
            | "shader"
            | "shaders"
            | "playfx"
            | "impactfx"
            | "deathfx"
            | "sounds"
            | "flags"
            | "spawnflags"
    )
}

fn skip_value(cursor: &mut Cursor<'_>) {
    match cursor.peek() {
        Some("{") => skip_balanced(cursor, "{", "}"),
        Some("[") => skip_balanced(cursor, "[", "]"),
        // A key with no value (the JoF HD `concussion/shot.efx` has a bare `linear` on its
        // own line inside `size { }`). Retail's line-based parser ignores it; never let it
        // swallow the closing brace of the block it sits in.
        Some("}" | "]") => {}
        Some(_) => {
            cursor.next();
            while cursor
                .peek()
                .is_some_and(|token| token.parse::<f32>().is_ok())
            {
                cursor.next();
            }
        }
        None => {}
    }
}

fn skip_balanced(cursor: &mut Cursor<'_>, open: &str, close: &str) {
    let mut depth = 0_u32;
    while let Some(token) = cursor.next() {
        if token == open {
            depth += 1;
        } else if token == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                break;
            }
        }
    }
}

fn tokenize(source: &str) -> Vec<String> {
    let mut clean = String::with_capacity(source.len());
    for line in source.lines() {
        clean.push_str(line.split_once("//").map_or(line, |(before, _)| before));
        clean.push('\n');
    }
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in clean.chars() {
        if character.is_whitespace() || matches!(character, '{' | '}' | '[' | ']') {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            if matches!(character, '{' | '}' | '[' | ']') {
                tokens.push(character.to_string());
            }
        } else {
            current.push(character);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

pub(crate) struct Cursor<'a> {
    tokens: &'a [String],
    position: usize,
}

impl<'a> Cursor<'a> {
    fn new(tokens: &'a [String]) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    pub(crate) fn next(&mut self) -> Option<&'a str> {
        let value = self.tokens.get(self.position)?;
        self.position += 1;
        Some(value)
    }

    pub(crate) fn peek(&self) -> Option<&str> {
        self.tokens.get(self.position).map(String::as_str)
    }

    pub(crate) fn peek_after(&self) -> Option<&str> {
        self.tokens.get(self.position + 1).map(String::as_str)
    }

    fn expect(&mut self, expected: &str) -> Result<(), EffectError> {
        let actual = self.next();
        if actual == Some(expected) {
            Ok(())
        } else {
            Err(EffectError::Parse(format!(
                "expected {expected:?}, found {actual:?}"
            )))
        }
    }

    fn number(&mut self) -> Result<f32, EffectError> {
        let token = self
            .next()
            .ok_or_else(|| EffectError::Parse("expected number at end of effect".into()))?;
        token
            .parse()
            .map_err(|_| EffectError::Parse(format!("expected number, found {token:?}")))
    }
}

#[cfg(test)]
mod tests {
    use crate::{ComponentKind, Range, parse_effect};

    /// The retail `effects/mp/drain.efx` (assets1.pk3), byte for byte apart from tabs.
    const DRAIN: &str = "Electricity
{
	flags				useModel useBBox usePhysics
	spawnFlags			org2fromTrace
	count				1
	life				75
	bounce				0.8 2
	rgb
	{
		start			1 0 0
		end				1 0 0
	}
	size
	{
		start			3 7
		flags			linear
	}
	shaders
	[
		gfx/misc/blueLine
	]
}

Particle
{
	life				30
	rotation			0 360
	rgb
	{
		start			1 0 0
		end				0.502 0 0
	}
	size
	{
		start			14 26
		flags			random
	}
	shaders
	[
		gfx/misc/lightningFlash
	]
}
";

    fn single(value: f32) -> Range {
        Range {
            minimum: value,
            maximum: value,
        }
    }

    const BOUNCE_RANGE: Range = Range {
        minimum: 0.8,
        maximum: 2.0,
    };

    /// Drain's bolts get `bounce 0.8 2` as their jaggedness, as retail's shared
    /// `mElasticity` gives them (`FxTemplate.cpp:2128`, `FxScheduler.cpp:1502-1508`),
    /// not the 0.1 default that drew them almost straight.
    #[test]
    fn drain_bounce_is_bolt_jaggedness() {
        let effect = parse_effect(DRAIN).unwrap();
        let bolt = &effect.components[0];
        assert_eq!(bolt.kind, ComponentKind::Electricity);
        assert_eq!(bolt.chaos, BOUNCE_RANGE);
        assert!(bolt.chaos_authored);
        assert_eq!(bolt.elasticity, BOUNCE_RANGE);
        assert!(bolt.flags.apply_physics);
        assert!(bolt.flags.use_model);
        // The flash keeps the retail default of 0.1.
        let flash = &effect.components[1];
        assert_eq!(flash.kind, ComponentKind::Particle);
        assert_eq!(flash.chaos, single(0.1));
        assert_eq!(flash.elasticity, single(0.1));
        assert!(!flash.flags.apply_physics);
    }

    /// `intensity` is the same key as `bounce`: it also sets the bounce and turns on
    /// physics (`ParseElasticity`, `FxTemplate.cpp:448-458`).
    #[test]
    fn intensity_and_bounce_are_one_key() {
        let bounce = parse_effect("Particle { bounce 0.25 0.5 }").unwrap();
        let intensity = parse_effect("Particle { intensity 0.25 0.5 }").unwrap();
        for component in [&bounce.components[0], &intensity.components[0]] {
            let range = Range {
                minimum: 0.25,
                maximum: 0.5,
            };
            assert_eq!(component.elasticity, range);
            assert_eq!(component.chaos, range);
            assert_eq!(component.intensity, range);
            assert!(component.flags.apply_physics);
        }
        let shake = parse_effect("CameraShake { intensity 3 }").unwrap();
        assert_eq!(shake.components[0].intensity, single(3.0));
    }

    /// Retail has no `elasticity` or `chaos` key; it skips them as unknown.
    #[test]
    fn non_retail_keys_are_ignored() {
        let effect = parse_effect("Electricity { elasticity 0.9 chaos 3 count 2 }").unwrap();
        let bolt = &effect.components[0];
        assert_eq!(bolt.chaos, single(0.1));
        assert!(!bolt.flags.apply_physics);
        assert_eq!(bolt.count, single(2.0));
    }

    /// JoF HD Weapon Effects `effects/concussion/shot.efx`: a bare `linear` key sits on its
    /// own line inside the Light's `size` block. It used to eat the block's closing brace and
    /// the whole effect failed with "unterminated effect component" (no concussion trail).
    const HD_CONCUSSION_LIGHT: &str = "repeatDelay 300\r\nLight\r\n{\r\n\trgb\r\n\t{\r\n\t\tstart 0.1882 0 0.749 0.6863 0.1686 1\r\n\t}\r\n\tsize\r\n\t{\r\n\t\tstart 80 90\r\n\t\tlinear\r\n\t}\r\n}\r\nParticle\r\n{\r\n\tsize\r\n\t{\r\n\t\tstart 12 13\r\n\t}\r\n}";

    #[test]
    fn bare_key_does_not_swallow_closing_brace() {
        let effect = parse_effect(HD_CONCUSSION_LIGHT).expect("HD concussion shot parses");
        assert_eq!(effect.components.len(), 2);
        assert_eq!(effect.components[0].size.start.minimum, 80.0);
        assert_eq!(effect.components[0].size.start.maximum, 90.0);
        assert_eq!(effect.components[1].size.start.minimum, 12.0);
    }
}
