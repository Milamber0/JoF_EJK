//! Map entities (`r_showEntities`): lights, spawns, origins, items and the other
//! point entities a map file places, marked over the view through walls with their
//! class names, so they can be found and picked with a world note. The brushes are
//! `r_showBrushes`' ([`crate::brush_view`]).
//!
//! The markers come from the BSP's entity lump, read once per map the first time the
//! view is on. A marker is a coloured square on the entity's origin; the nearest ones
//! carry their class name. [`Marker::aim`] is the world note's pick: the marker nearest
//! the view ray within a few pixels beats the wall behind it.

use crate::GpuState;
use crate::hud::identification::Camera;
use crate::world_ping::screen_place;
use glam::Vec3;
use sjk_ui::{Color, DrawCommand, DrawList, FontWeight, Rect, TextAlign, TextId, TextOverflow};

/// 0 off, 1 the usual suspects (lights, spawns, origins, items, targets), 2 every kind.
pub(crate) const CVAR: &str = "r_showEntities";
/// Kind names that replace the set `r_showEntities` picks, when any is listed.
pub(crate) const TYPES_CVAR: &str = "r_showEntitiesTypes";

/// Farthest a marker shows, in map units.
const REACH: f32 = 6000.0;
/// Most markers drawn, and most of them with a label (the nearest).
const MAX_MARKS: usize = 220;
const MAX_LABELS: usize = 60;
/// Farthest a label shows.
const LABEL_REACH: f32 = 1500.0;
/// How far from a marker the view ray may pass and still pick it, as a fraction of the
/// distance (about 15 pixels at 90 degrees and 1080p).
const AIM_ANGLE: f32 = 0.025;
/// Half the side of the box a picked marker is outlined with.
const BOX_HALF: f32 = 8.0;

/// One kind of point entity, drawn with its own colour.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    /// `light` and the `light_*` entities.
    Light,
    /// Player, flag and Jedi-master spawns and the intermission camera.
    Spawn,
    /// Pure positions: `info_null`, `target_position`, `path_corner`...
    Origin,
    /// `item_*`, `weapon_*` and `ammo_*` pickups.
    Item,
    /// Any other `target_*` entity.
    Target,
    /// `target_speaker` and `ambient_*` sounds.
    Sound,
    /// `fx_*` effect emitters.
    Effect,
    /// `misc_*` entities, props and the like.
    Misc,
    /// Any other entity with an origin and no brushes.
    Other,
}

impl Kind {
    pub(crate) const ALL: [Self; 9] = [
        Self::Light,
        Self::Spawn,
        Self::Origin,
        Self::Item,
        Self::Target,
        Self::Sound,
        Self::Effect,
        Self::Misc,
        Self::Other,
    ];

    /// What `r_showEntities 1` shows.
    const COMMON: u16 = 1 << Self::Light as u16
        | 1 << Self::Spawn as u16
        | 1 << Self::Origin as u16
        | 1 << Self::Item as u16
        | 1 << Self::Target as u16;

    /// The kind's name in `r_showEntitiesTypes`.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Spawn => "spawn",
            Self::Origin => "origin",
            Self::Item => "item",
            Self::Target => "target",
            Self::Sound => "sound",
            Self::Effect => "effect",
            Self::Misc => "misc",
            Self::Other => "other",
        }
    }

    /// Display (sRGB) colour of the marker.
    fn colour(self) -> [f32; 3] {
        match self {
            Self::Light => [1.0, 0.92, 0.30],
            Self::Spawn => [0.30, 0.90, 0.40],
            Self::Origin => [0.30, 0.80, 1.0],
            Self::Item => [1.0, 0.55, 0.15],
            Self::Target => [0.95, 0.35, 0.60],
            Self::Sound => [0.70, 0.55, 1.0],
            Self::Effect => [0.40, 1.0, 0.85],
            Self::Misc => [0.80, 0.80, 0.85],
            Self::Other => [0.60, 0.60, 0.60],
        }
    }

    /// The kind of an entity by classname (`codemp/game/g_spawn.c`'s spawn table).
    pub(crate) fn of_class(classname: &str) -> Self {
        let class = classname.to_ascii_lowercase();
        let starts = |prefix: &str| class.starts_with(prefix);
        if starts("light") {
            Self::Light
        } else if starts("info_player_")
            || starts("team_ctf_")
            || starts("info_jedimaster")
            || class == "info_player_intermission"
        {
            Self::Spawn
        } else if matches!(
            class.as_str(),
            "info_null" | "info_notnull" | "target_position" | "target_location" | "ref_tag"
        ) || starts("path_corner")
        {
            Self::Origin
        } else if starts("item_") || starts("weapon_") || starts("ammo_") {
            Self::Item
        } else if class == "target_speaker" || starts("ambient_") {
            Self::Sound
        } else if starts("fx_") {
            Self::Effect
        } else if starts("target_") {
            Self::Target
        } else if starts("misc_") {
            Self::Misc
        } else {
            Self::Other
        }
    }
}

/// A set of [`Kind`]s, one bit each.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct KindSet(u16);

impl KindSet {
    pub(crate) const EMPTY: Self = Self(0);

    pub(crate) fn contains(self, kind: Kind) -> bool {
        self.0 & 1 << kind as u16 != 0
    }

    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The kinds `r_showEntities` `mode` and `r_showEntitiesTypes` `types` show:
    /// `types`' names when it lists any, else the common kinds for mode 1 and every
    /// kind for 2 and above. Unknown names are ignored.
    pub(crate) fn chosen(mode: i64, types: &str) -> Self {
        if mode <= 0 {
            return Self::EMPTY;
        }
        let listed = types
            .split(|c: char| c.is_ascii_whitespace() || c == ',' || c == ';')
            .filter_map(|name| {
                Kind::ALL
                    .into_iter()
                    .find(|kind| kind.name().eq_ignore_ascii_case(name))
            })
            .fold(0, |bits, kind| bits | 1 << kind as u16);
        Self(if listed != 0 {
            listed
        } else if mode == 1 {
            Kind::COMMON
        } else {
            (1 << Kind::ALL.len()) - 1
        })
    }
}

/// One point entity of the map.
#[derive(Clone, Debug)]
pub(crate) struct Marker {
    /// Its index in the entity lump.
    pub(crate) number: usize,
    pub(crate) origin: Vec3,
    pub(crate) kind: Kind,
    /// What the label says: the class name, its target name and a light's intensity.
    label: String,
}

impl Marker {
    /// The marker of the entity `number`, when it is a point entity with an origin.
    fn of(number: usize, entity: &sjk_entity::Entity) -> Option<Self> {
        let classname = entity.classname()?;
        // Brush entities draw as brushes (`*N` models).
        if entity.get("model").is_some_and(|m| m.starts_with('*')) {
            return None;
        }
        let origin = Vec3::from_array(entity.vector("origin").ok().flatten()?);
        let kind = Kind::of_class(classname);
        let mut label = classname.to_owned();
        if let Some(name) = entity.get("targetname").filter(|name| !name.is_empty()) {
            label.push_str(" \"");
            label.push_str(name);
            label.push('"');
        }
        if kind == Kind::Light
            && let Some(light) = entity.get("light").filter(|light| !light.is_empty())
        {
            label.push(' ');
            label.push_str(light);
        }
        Some(Self {
            number,
            origin,
            kind,
            label,
        })
    }

    /// The marker of `markers` nearest the ray from `eye` along `direction` within a
    /// few pixels of it, with its distance along the ray.
    pub(crate) fn aim(markers: &[Self], eye: Vec3, direction: Vec3) -> Option<(&Self, f32)> {
        markers
            .iter()
            .filter_map(|marker| {
                let along = (marker.origin - eye).dot(direction);
                if along <= 1.0 || along > REACH {
                    return None;
                }
                let off = (marker.origin - (eye + direction * along)).length();
                let angle = off / along;
                (angle <= AIM_ANGLE).then_some((marker, along, angle))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(marker, along, _)| (marker, along))
    }

    /// The corners of the box a picked marker is outlined with.
    pub(crate) fn bounds(&self) -> (Vec3, Vec3) {
        (
            self.origin - Vec3::splat(BOX_HALF),
            self.origin + Vec3::splat(BOX_HALF),
        )
    }
}

/// The markers of the map and this frame's drawing of them.
pub(crate) struct EntityView {
    /// The entity lump the markers were read from: its address and length.
    source: (usize, usize),
    markers: Vec<Marker>,
    shown: KindSet,
    list: DrawList,
    labels: Vec<String>,
}

impl Default for EntityView {
    fn default() -> Self {
        Self {
            source: (0, 0),
            markers: Vec::new(),
            shown: KindSet::EMPTY,
            list: DrawList::new(MAX_MARKS * 3 + MAX_LABELS),
            labels: Vec::new(),
        }
    }
}

impl EntityView {
    /// Whether this frame draws anything.
    pub(crate) fn active(&self) -> bool {
        !self.list.is_empty()
    }

    /// The markers of the shown kinds: what a world note can pick.
    pub(crate) fn pickable(&self) -> Vec<Marker> {
        self.markers
            .iter()
            .filter(|marker| self.shown.contains(marker.kind))
            .cloned()
            .collect()
    }

    pub(crate) fn list(&self) -> &DrawList {
        &self.list
    }

    pub(crate) fn text(&self, id: TextId) -> &str {
        self.labels.get(id.0 as usize).map_or("", String::as_str)
    }

    /// Read the markers of `bsp` when they are not those of this map, then lay out
    /// the shown kinds for `camera` (nothing when not `visible`).
    fn build(
        &mut self,
        bsp: &sjk_bsp::Bsp,
        wanted: KindSet,
        camera: Option<Camera>,
        visible: bool,
    ) {
        self.list.clear();
        self.labels.clear();
        self.shown = wanted;
        if wanted.is_empty() {
            return;
        }
        let lump = bsp.entities();
        let source = (lump.as_ptr() as usize, lump.len());
        if source != self.source {
            self.source = source;
            self.markers = sjk_entity::parse_entity_lump(lump)
                .unwrap_or_default()
                .iter()
                .enumerate()
                .skip(1)
                .filter_map(|(number, entity)| Marker::of(number, entity))
                .collect();
        }
        let Some(camera) = camera.filter(|_| visible) else {
            return;
        };
        let unit = crate::ui_scale::height_scale(camera.viewport[1]);
        let mut near: Vec<(f32, [f32; 2], &Marker)> = self
            .markers
            .iter()
            .filter(|marker| wanted.contains(marker.kind))
            .filter_map(|marker| {
                let distance = marker.origin.distance(camera.eye);
                if distance > REACH {
                    return None;
                }
                let (at, on_screen) = screen_place(camera, marker.origin, 0.0)?;
                on_screen.then_some((distance, at, marker))
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(MAX_MARKS);
        let side = 8.0 * unit;
        let size = 15.0 * unit;
        for (rank, (distance, at, marker)) in near.into_iter().enumerate() {
            let [r, g, b] = marker.kind.colour();
            let fade = (1.0 - distance / REACH).clamp(0.0, 1.0).sqrt();
            let alpha = 0.55 + 0.45 * fade;
            let back = side + 4.0 * unit;
            let _ = self.list.push(DrawCommand::SolidRect {
                rect: Rect::new(at[0] - back * 0.5, at[1] - back * 0.5, back, back),
                color: Color::new(0.0, 0.0, 0.0, 0.6 * alpha),
            });
            let _ = self.list.push(DrawCommand::SolidRect {
                rect: Rect::new(at[0] - side * 0.5, at[1] - side * 0.5, side, side),
                color: Color::new(r, g, b, alpha),
            });
            if rank < MAX_LABELS && distance <= LABEL_REACH {
                let id = u32::try_from(self.labels.len()).unwrap_or(0);
                self.labels.push(marker.label.clone());
                let width = 320.0 * unit;
                let _ = self.list.push(DrawCommand::Text {
                    rect: Rect::new(
                        (at[0] - width * 0.5).clamp(0.0, (camera.viewport[0] - width).max(0.0)),
                        at[1] + back * 0.5 + 2.0 * unit,
                        width,
                        size * 1.4,
                    ),
                    text: TextId(id),
                    size,
                    color: Color::new(r, g, b, alpha),
                    align: TextAlign::Center,
                    overflow: TextOverflow::Ellipsis,
                    weight: FontWeight::Regular,
                    letter_spacing: 0.0,
                });
            }
        }
    }
}

impl GpuState {
    /// Lay the entity markers out over this frame's view and add their labels to the
    /// HUD's text; `shown` is false in menus, the console and the like (the markers can
    /// still be picked by a world note).
    pub(crate) fn append_entity_view(&mut self, viewport: [f32; 2], shown: bool) {
        let console = self.console.as_ref();
        let wanted = if self.is_menu_world || self.world_hidden {
            KindSet::EMPTY
        } else {
            KindSet::chosen(
                console.and_then(|c| c.integer_cvar(CVAR)).unwrap_or(0),
                console.and_then(|c| c.text_value(TYPES_CVAR)).unwrap_or(""),
            )
        };
        self.entity_view
            .build(&self.bsp, wanted, self.frame_view, shown);
        if !self.entity_view.active() {
            return;
        }
        let view = &self.entity_view;
        crate::ui_renderer::append_text_commands(
            &view.list,
            |id| view.text(id),
            &mut self.text_vertices,
            &self.ui_font,
            viewport,
            crate::text::TextStyle::NEUTRAL,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(fields: &[(&str, &str)]) -> sjk_entity::Entity {
        sjk_entity::Entity::from_fields(
            fields
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        )
    }

    #[test]
    fn classes_get_their_kinds() {
        assert_eq!(Kind::of_class("light"), Kind::Light);
        assert_eq!(Kind::of_class("info_player_deathmatch"), Kind::Spawn);
        assert_eq!(Kind::of_class("team_CTF_redplayer"), Kind::Spawn);
        assert_eq!(Kind::of_class("info_null"), Kind::Origin);
        assert_eq!(Kind::of_class("path_corner"), Kind::Origin);
        assert_eq!(Kind::of_class("weapon_bryar_pistol"), Kind::Item);
        assert_eq!(Kind::of_class("target_speaker"), Kind::Sound);
        assert_eq!(Kind::of_class("target_relay"), Kind::Target);
        assert_eq!(Kind::of_class("fx_runner"), Kind::Effect);
        assert_eq!(Kind::of_class("misc_model_static"), Kind::Misc);
        assert_eq!(Kind::of_class("something_else"), Kind::Other);
    }

    #[test]
    fn modes_and_type_lists_choose_the_kinds() {
        assert!(KindSet::chosen(0, "light").is_empty());
        let common = KindSet::chosen(1, "");
        assert!(common.contains(Kind::Light) && common.contains(Kind::Spawn));
        assert!(!common.contains(Kind::Misc));
        let all = KindSet::chosen(2, " ");
        assert!(Kind::ALL.into_iter().all(|kind| all.contains(kind)));
        let listed = KindSet::chosen(1, "Misc, sound;bogus");
        assert!(listed.contains(Kind::Misc) && listed.contains(Kind::Sound));
        assert!(!listed.contains(Kind::Light));
    }

    #[test]
    fn point_entities_become_markers_and_brush_entities_do_not() {
        let light = entity(&[
            ("classname", "light"),
            ("origin", "10 20 30"),
            ("light", "300"),
        ]);
        let marker = Marker::of(3, &light).expect("a light is a marker");
        assert_eq!(marker.origin, Vec3::new(10.0, 20.0, 30.0));
        assert_eq!(marker.label, "light 300");
        let door = entity(&[
            ("classname", "func_door"),
            ("model", "*4"),
            ("origin", "0 0 0"),
        ]);
        assert!(Marker::of(4, &door).is_none());
        assert!(Marker::of(5, &entity(&[("classname", "info_null")])).is_none());
        let named = entity(&[
            ("classname", "target_position"),
            ("targetname", "t1"),
            ("origin", "0 0 0"),
        ]);
        assert_eq!(
            Marker::of(6, &named).unwrap().label,
            "target_position \"t1\""
        );
    }

    #[test]
    fn aim_takes_the_marker_nearest_the_ray() {
        let at = |x: f32, y: f32| Marker {
            number: 1,
            origin: Vec3::new(x, y, 0.0),
            kind: Kind::Light,
            label: String::new(),
        };
        let markers = [at(500.0, 20.0), at(1000.0, 5.0), at(-200.0, 0.0)];
        let (marker, along) = Marker::aim(&markers, Vec3::ZERO, Vec3::X).expect("one is aimed at");
        assert_eq!(marker.origin.x, 1000.0);
        assert_eq!(along, 1000.0);
        assert!(Marker::aim(&markers[..1], Vec3::ZERO, Vec3::Y).is_none());
    }
}
