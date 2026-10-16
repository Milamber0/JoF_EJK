//! Reference tags (`ref_tag`, `ref_tag_huge`; `g_misc.c:3100-3470`): named places and
//! angles a map sets out for its scripts — `move(tag("level_2a", ORIGIN), ...)`.
//!
//! A tag belongs to an owner (the map's `ownername` key, or the generic owner). Names
//! are kept in lower case and found in any case. A script asks under its entity's own
//! `ownername` first and then the generic owner (`TAG_Find`). The tag entity itself is
//! freed as soon as it is filed; a tag that `target`s an entity takes its angles from the
//! direction to it once every entity has spawned.
//!
//! The reference keeps at most 16 owners of 256 tags each in fixed arrays; these grow as
//! the map needs, a map's tags being bounded by the map.

/// The owner of tags no one owns (`TAG_GENERIC_NAME`).
pub const GENERIC_OWNER: &str = "__WORLD__";
/// `MAX_REFNAME`: names are cut to this less one.
pub const MAX_REFNAME: usize = 32;

/// One tag (`reference_tag_t`).
#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    /// Its name, lower-cased.
    pub name: String,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub radius: i32,
    pub flags: i32,
}

/// Every tag of a level (`refTagOwnerMap`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RefTags {
    /// Owners, lower-cased, with their tags, in the order they were made.
    owners: Vec<(String, Vec<Tag>)>,
}

/// What filing a tag answers, as the reference prints it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Filed {
    /// The tag is filed.
    Added,
    /// `"Duplicate tag name \"%s\"\n"`: one by that name is found already.
    Duplicate,
    /// `"ERROR: Nameless ref_tag found at (%i %i %i)\n"`.
    Nameless,
}

/// `Q_strncpyz` into a `MAX_REFNAME` buffer, then `Q_strlwr`.
fn filed_name(name: &str) -> String {
    let mut end = name.len().min(MAX_REFNAME - 1);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].to_ascii_lowercase()
}

impl RefTags {
    fn owner(&self, owner: &str) -> Option<&(String, Vec<Tag>)> {
        self.owners
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(owner))
    }

    /// `TAG_Find`: the tag under the owner (or the generic owner when the owner has
    /// none or is unknown), then under the generic owner.
    pub fn find(&self, owner: Option<&str>, name: &str) -> Option<&Tag> {
        let owned = owner
            .filter(|owner| !owner.is_empty())
            .and_then(|owner| self.owner(owner))
            .or_else(|| self.owner(GENERIC_OWNER))?;
        fn named<'a>(list: &'a (String, Vec<Tag>), name: &str) -> Option<&'a Tag> {
            list.1
                .iter()
                .find(|tag| tag.name.eq_ignore_ascii_case(name))
        }
        named(owned, name).or_else(|| self.owner(GENERIC_OWNER).and_then(|list| named(list, name)))
    }

    /// `TAG_Add`: a tag filed under its owner (the generic one without), unless one by
    /// that name is found already or it has no name.
    pub fn add(
        &mut self,
        name: &str,
        owner: Option<&str>,
        origin: [f32; 3],
        angles: [f32; 3],
        radius: i32,
        flags: i32,
    ) -> Filed {
        if self.find(owner, name).is_some() {
            return Filed::Duplicate;
        }
        if name.is_empty() {
            // The reference has taken a free owner and a free tag and marks neither.
            return Filed::Nameless;
        }
        let owner = owner
            .filter(|owner| !owner.is_empty())
            .unwrap_or(GENERIC_OWNER);
        let index = match self
            .owners
            .iter()
            .position(|(existing, _)| existing.eq_ignore_ascii_case(owner))
        {
            Some(index) => index,
            None => {
                self.owners.push((String::new(), Vec::new()));
                self.owners.len() - 1
            }
        };
        self.owners[index].0 = filed_name(owner);
        self.owners[index].1.push(Tag {
            name: filed_name(name),
            origin,
            angles,
            radius,
            flags,
        });
        Filed::Added
    }

    /// `TAG_GetOrigin`: the tag's origin; `None` (and the vector cleared) without it.
    pub fn origin(&self, owner: Option<&str>, name: &str) -> Option<[f32; 3]> {
        self.find(owner, name).map(|tag| tag.origin)
    }

    /// `TAG_GetAngles`.
    pub fn angles(&self, owner: Option<&str>, name: &str) -> Option<[f32; 3]> {
        self.find(owner, name).map(|tag| tag.angles)
    }

    /// Forgets every tag (`TAG_Init`, at a level's start).
    pub fn clear(&mut self) {
        self.owners.clear();
    }
}

/// A `ref_tag` as the map places it: what `SP_reference_tag` files.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedTag {
    pub targetname: String,
    pub ownername: Option<String>,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    /// `target`: the entity whose direction gives its angles, once everything spawned
    /// (`ref_link` as a think at `START_TIME_LINK_ENTS`).
    pub target: Option<String>,
}

/// `SP_reference_tag`'s reading of a map entity: `None` for any other classname.
pub fn placed(entity: &sjk_entity::Entity) -> Option<PlacedTag> {
    if !matches!(entity.get("classname"), Some("ref_tag" | "ref_tag_huge")) {
        return None;
    }
    let text = |key: &str| {
        entity
            .get(key)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let angles = match entity.vector("angles").ok().flatten() {
        Some(angles) => angles,
        None => [
            0.0,
            entity
                .get("angle")
                .map_or(0.0, |angle| crate::text_parse::atof(angle.as_bytes())),
            0.0,
        ],
    };
    Some(PlacedTag {
        targetname: entity.get("targetname").unwrap_or_default().to_owned(),
        ownername: text("ownername"),
        origin: entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]),
        angles,
        target: text("target"),
    })
}

/// `ref_link` for a tag that targets something at `target`: its angles face that way.
pub fn aimed_angles(tag: &PlacedTag, target: [f32; 3]) -> [f32; 3] {
    let mut direction: [f32; 3] = std::array::from_fn(|axis| target[axis] - tag.origin[axis]);
    crate::player_angle_math::normalize(&mut direction);
    crate::player_angle_math::vector_angles(direction)
}
