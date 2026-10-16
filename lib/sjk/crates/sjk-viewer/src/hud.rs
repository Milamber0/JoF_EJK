//! Data-driven in-game HUD over authoritative snapshot values.

mod data_source;
use data_source::*;
pub(crate) mod crosshair;
pub(crate) mod enemy_info;
mod estimate;
pub(crate) mod family;
mod force_estimate;
mod force_streams;
pub(crate) mod force_wheel;
pub(crate) mod icons;
pub(crate) mod identification;
mod info;
pub(crate) mod kill_feed;
pub(crate) mod movement;
pub(crate) mod nameplate;
mod nameplate_math;
mod npc_class;
pub(crate) mod options;
pub(crate) mod player_card;
pub(crate) mod portrait;
mod radial;
pub(crate) mod rocket_lock;
mod selection;
pub(crate) mod targeting;
pub(crate) mod team_logo;
pub(crate) mod team_outline;
mod text_values;
pub(crate) mod tints;
mod update;
pub(crate) mod vehicle;
pub(crate) mod vehicle_feed;
mod vitals_estimate;
mod vote;
mod widgets;

use super::{TextVertex, UiFont};
use crate::game_font::RetailFont;
use crate::{console::ViewerConsole, ui_renderer};
use sjk_client::pmove::MovementState;
use sjk_client::{
    ClientSession, HudDataSource as ClientHudData, legacy_hud_data, legacy_predicted_hud_data,
};
use sjk_protocol::{GameState, PlayerState};
use sjk_ui::{
    DrawList, Easing, HudDataSource, HudLayoutDocument, Insets, LayoutContext, LayoutEngine,
    LayoutKind, LayoutScratch, TextId, Theme, Tween, Vec2, Widget, WidgetId, WidgetTree,
};
use std::path::Path;

const WIDGET_LIMIT: usize = 64;
/// The HUD's draw commands: the 384 its other widgets had, and room for a full
/// kill feed (55).
const DRAW_LIMIT: usize = 384 + kill_feed::CAPACITY * kill_feed::ENTRY_COMMANDS;
const DEFAULT_LAYOUT: &str = include_str!("../assets/hud/default.json");
/// The weapon name fades over this long at the end of its life.
const WEAPON_FADE_MS: u64 = 600;
/// The weapon name stays fully visible this long after a switch, then fades. Name and
/// fade together last as long as the weapon selection row (`WEAPON_SELECT_TIME`), which
/// replaces the name while it shows, so the name never appears after the row ends.
const WEAPON_HOLD_MS: u64 = crate::weapon_select::SHOW.as_millis() as u64 - WEAPON_FADE_MS;
/// Opacity of a transient that holds for `hold` ms and fades over `fade` ms.
fn transient_alpha(age_ms: u64, hold: u64, fade: u64) -> f32 {
    if age_ms <= hold {
        1.0
    } else {
        1.0 - (age_ms - hold).min(fade) as f32 / fade as f32
    }
}
const CLASSIC_LAYOUT: &str = include_str!("../assets/hud/classic.json");
const RADIAL_LAYOUT: &str = include_str!("../assets/hud/radial.json");

/// Independent visibility switches used by widget predicates and the HUD shader.
#[derive(Clone, Copy)]
pub(crate) struct HudVisibility {
    pub(crate) hud: bool,
    pub(crate) status: bool,
    pub(crate) weapon: bool,
    pub(crate) crosshair: bool,
    pub(crate) crosshair_names: bool,
    pub(crate) timer: bool,
    pub(crate) lagometer: bool,
    /// The third-person ground HUD stands in for health, shield, Force and
    /// stance this frame (`crate::ground_hud`), so those widgets hide.
    pub(crate) ground_hud: bool,
    /// The game-data menu HUD (`crate::menu_hud`) draws health, armor,
    /// Force and ammo this frame, so the built-in status and weapon widgets hide.
    pub(crate) menu_hud: bool,
}

impl HudVisibility {
    pub(crate) fn from_console(console: Option<&ViewerConsole>) -> Self {
        let enabled = |name, fallback| {
            console
                .and_then(|console| console.bool_cvar(name))
                .unwrap_or(fallback)
        };
        let hud = enabled("cg_drawHud", true) && enabled("cg_draw2D", true);
        Self {
            hud,
            status: hud && enabled("cg_drawStatus", true),
            weapon: hud && enabled("cg_drawWeapon", true),
            crosshair: hud
                && console
                    .and_then(|c| c.integer_cvar("cg_drawCrosshair"))
                    .unwrap_or(1)
                    != 0,
            crosshair_names: hud && enabled("cg_drawCrosshairNames", true),
            timer: hud && enabled("cg_drawTimer", true),
            lagometer: hud && enabled("cg_lagometer", false),
            ground_hud: false,
            menu_hud: false,
        }
    }

    /// Intermission hides every HUD layer.
    pub(crate) const HIDDEN: Self = Self {
        hud: false,
        status: false,
        weapon: false,
        crosshair: false,
        crosshair_names: false,
        timer: false,
        lagometer: false,
        ground_hud: false,
        menu_hud: false,
    };
}

/// Allocation-free-per-frame widget state for the local player's HUD.
pub(crate) struct HudOverlay {
    /// Map-lifetime textures and sampled icon presentation policy.
    pub(crate) icons: icons::Icons,
    pub(crate) tints: tints::State,
    /// World-projected labels, sharing the chat roster's retained names.
    pub(crate) identification: identification::State,
    /// The card shown beside a player looked at for a moment.
    pub(crate) card: player_card::State,
    /// MMO-style nameplates, drawn in the classic font; they replace the labels above.
    pub(crate) nameplate: nameplate::State,
    /// Team logos over allies' heads, under the labels and nameplates.
    pub(crate) team_logo: team_logo::State,
    /// Who the team outlines ring this frame.
    pub(crate) team_outline: team_outline::State,
    guides: movement::Guides,
    family: family::Policy,
    pub(crate) targeting: targeting::State,
    pub(crate) enemy_info: enemy_info::State,
    /// The pilot's vehicle HUD and the crosshair it changes.
    pub(crate) vehicle: vehicle::State,
    score_text: String,
    snapshot_text: String,

    inventory_bits: u32,
    selector: Option<sjk_client::selection::SelectionView>,
    /// The Force selector is the retail icon bar ([`force_wheel`]), not the list.
    force_wheel_bar: bool,
    /// The bar's map-lifetime pictures.
    force_wheel_icons: [Option<sjk_ui::TextureId>; force_wheel::ICONS],
    /// The holocron tiers' pictures in the UI atlas (`holocrons/icons.rs`), where they
    /// loaded.
    pub(crate) holocron_icons: [Option<sjk_ui::TextureId>; crate::holocrons::COUNT],
    /// The classic crosshair's map-lifetime pictures.
    pub(crate) crosshair_pictures: [Option<sjk_ui::TextureId>; crosshair::PICTURES],
    /// The last layout drew the crosshair picture, so `hud.wgsl` skips its own.
    pub(crate) crosshair_picture_drawn: bool,
    /// The rocket launcher's lock-on around its target.
    pub(crate) rocket_lock: rocket_lock::State,
    /// JA+ merc mode's flamethrower, latched across snapshots.
    flamethrower: sjk_client::force_wheel::FlamethrowerOverride,
    /// The selector shows Force Lightning as the flamethrower.
    flamethrower_shown: bool,

    values: Option<ClientHudData>,
    health: String,
    armor: String,
    force: String,
    weapon: String,
    ammo: String,
    health_value: String,
    armor_value: String,
    force_value: String,
    weapon_value: String,
    ammo_value: String,
    style_value: String,
    /// HUD time of the last weapon change; drives the weapon-name fade.
    weapon_shown_ms: Option<u64>,
    /// Retail's weapon selection row (`crate::weapon_select`) and its names.
    pub(crate) weapon_select: crate::weapon_select::State,
    weapon_alpha: f32,
    vote_active: bool,
    team_vote_active: bool,
    vote_heading: String,
    vote_text: String,
    team_vote_heading: String,
    team_vote_text: String,
    vote_keys: String,
    yes_keys: String,
    no_keys: String,
    /// The kill feed at the top right.
    pub(crate) kill_feed: kill_feed::Feed,
    crosshair_name: String,
    speed: options::Speed,
    crosshair_alpha: f32,
    crosshair_teammate: bool,
    match_timer: String,
    /// Whether the match clock shows in the upper right this frame (`cg_drawTimer`,
    /// `cg_drawUpperRight`, a clock to show), as JoF EJK's `CG_DrawUpperRight` puts it.
    upper_right_timer: bool,
    warmup_text: String,
    interrupted: bool,
    lagometer: sjk_client::LagometerSamples,
    default_document: HudLayoutDocument,
    classic_document: HudLayoutDocument,
    override_document: Option<HudLayoutDocument>,
    tree: WidgetTree,
    scratch: LayoutScratch,
    draw_list: DrawList,
    theme: Theme,
    ratios: [Tween; 3],
    displayed_ratios: [f32; 3],
    ammo_ratio: Tween,
    displayed_ammo: f32,
    radial_document: HudLayoutDocument,
}

/// Which bundled layout document the status HUD is drawn from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HudLook {
    /// Under the game-data HUD: the default layout (or a `hud.json` override),
    /// whose crosshair, team rows, votes, kill feed, timer and lagometer stay
    /// while the game HUD draws the status, and whose status widgets stand in
    /// when its files give none; the classic layout with `cg_classicHudFont`.
    Game,
    /// SJK's classic layout.
    Classic,
    /// SJK's radial layout: arcs around the crosshair.
    Radial,
}

/// Pixel-space geometry consumed by the existing fullscreen HUD renderer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct HudLayout {
    pub(crate) health_bar: [f32; 4],
    pub(crate) armor_bar: [f32; 4],
    pub(crate) force_bar: [f32; 4],
}

impl HudOverlay {
    /// The Force power pictures, by `forcePowers_t` index, which the nameplates reuse.
    pub(crate) fn power_icons(&self) -> [Option<sjk_ui::TextureId>; force_wheel::ICONS] {
        self.force_wheel_icons
    }

    /// Whether the Force selector shows Lightning as JA+ merc mode's flamethrower.
    pub(crate) fn flamethrower_shown(&self) -> bool {
        self.flamethrower_shown
    }

    /// How much larger the procedural crosshair is drawn while riding a vehicle.
    pub(crate) fn vehicle_crosshair_factor(&self) -> f32 {
        self.vehicle.crosshair_factor(self.targeting.policy.look)
    }

    pub(crate) fn new() -> Self {
        let override_document = crate::platform::user_config_file()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.join("hud.json")))
            .and_then(|path| load_override(&path));
        Self {
            icons: icons::Icons::default(),
            weapon_select: crate::weapon_select::State::new(),
            tints: tints::State::default(),
            guides: movement::Guides::default(),
            family: family::Policy::default(),
            targeting: targeting::State::default(),
            enemy_info: enemy_info::State::default(),
            vehicle: vehicle::State::default(),
            identification: identification::State::default(),
            card: player_card::State::default(),
            nameplate: nameplate::State::default(),
            team_logo: team_logo::State::default(),
            team_outline: team_outline::State::default(),
            score_text: String::with_capacity(80),
            snapshot_text: String::with_capacity(96),

            inventory_bits: 0,
            values: None,
            health: String::with_capacity(24),
            armor: String::with_capacity(24),
            force: String::with_capacity(24),
            weapon: String::with_capacity(40),
            ammo: String::with_capacity(24),
            health_value: String::with_capacity(12),
            armor_value: String::with_capacity(12),
            force_value: String::with_capacity(12),
            weapon_value: String::with_capacity(32),
            ammo_value: String::with_capacity(12),
            style_value: String::with_capacity(12),
            weapon_shown_ms: None,
            weapon_alpha: 0.0,
            selector: None,
            force_wheel_bar: false,
            force_wheel_icons: [None; force_wheel::ICONS],
            holocron_icons: [None; crate::holocrons::COUNT],
            crosshair_pictures: [None; crosshair::PICTURES],
            rocket_lock: rocket_lock::State::default(),
            crosshair_picture_drawn: false,
            flamethrower: Default::default(),
            flamethrower_shown: false,

            vote_active: false,
            team_vote_active: false,
            vote_heading: String::with_capacity(64),
            vote_text: String::with_capacity(160),
            team_vote_heading: String::with_capacity(64),
            team_vote_text: String::with_capacity(160),
            vote_keys: String::with_capacity(96),
            yes_keys: String::with_capacity(48),
            no_keys: String::with_capacity(48),
            kill_feed: kill_feed::Feed::default(),
            crosshair_name: String::with_capacity(64),
            speed: options::Speed::default(),
            crosshair_alpha: 0.0,
            crosshair_teammate: false,
            match_timer: String::with_capacity(16),
            upper_right_timer: false,
            warmup_text: String::with_capacity(48),
            interrupted: false,
            lagometer: sjk_client::LagometerSamples::new(),
            default_document: HudLayoutDocument::from_json(DEFAULT_LAYOUT)
                .expect("bundled default HUD document is valid"),
            classic_document: HudLayoutDocument::from_json(CLASSIC_LAYOUT)
                .expect("bundled classic HUD document is valid"),
            override_document,
            tree: WidgetTree::new(WIDGET_LIMIT),
            scratch: LayoutScratch::new(WIDGET_LIMIT),
            draw_list: DrawList::new(DRAW_LIMIT),
            theme: Theme::default(),
            ratios: [
                Tween::settled(1.0),
                Tween::settled(0.0),
                Tween::settled(1.0),
            ],
            displayed_ratios: [1.0, 0.0, 1.0],
            ammo_ratio: Tween::settled(0.0),
            displayed_ammo: 0.0,
            radial_document: HudLayoutDocument::from_json(RADIAL_LAYOUT)
                .expect("bundled radial HUD document is valid"),
        }
    }

    /// Build, lay out and retain the HUD widget draw list. `user_scale` is
    /// the player's `cg_hudScale` on top of the resolution-derived scale.
    /// The match clock to show in the upper right under the frame rate, as JoF EJK's
    /// `CG_DrawTimer` does, when it shows ([`Self::layout`] decides each frame).
    pub(crate) fn upper_right_timer(&self) -> Option<&str> {
        self.upper_right_timer.then_some(self.match_timer.as_str())
    }

    pub(crate) fn layout(
        &mut self,
        font: &UiFont,
        look: HudLook,
        viewport: [f32; 2],
        user_scale: f32,
        visibility: HudVisibility,
        time_ms: u64,
    ) -> HudLayout {
        self.upper_right_timer = visibility.timer
            && self.family.upper
            && self.family.timer_upper_right
            && !self.match_timer.is_empty();
        self.displayed_ratios = self.ratios.map(|tween| tween.sample(time_ms));
        self.displayed_ammo = self.ammo_ratio.sample(time_ms);
        self.weapon_alpha = self.weapon_shown_ms.map_or(0.0, |shown| {
            transient_alpha(
                time_ms.saturating_sub(shown),
                WEAPON_HOLD_MS,
                WEAPON_FADE_MS,
            )
        });
        let document = match look {
            HudLook::Radial => &self.radial_document,
            HudLook::Classic => &self.classic_document,
            HudLook::Game if font.is_modern() => self
                .override_document
                .as_ref()
                .unwrap_or(&self.default_document),
            HudLook::Game => &self.classic_document,
        };
        // The nameplates' bars wear this HUD's health, armour and Force colours.
        let meter = |binding: &str| {
            document
                .widgets
                .iter()
                .find(|widget| widget.binding.as_deref() == Some(binding))
                .and_then(|widget| widget.style.foreground)
        };
        let (health, armor, force) = (
            meter("health_ratio"),
            meter("armor_ratio"),
            meter("force_ratio"),
        );
        self.nameplate.set_hud_colors(health, armor, force);
        let data = WidgetData {
            visibility,
            ratios: self.displayed_ratios,
            ammo_ratio: self.displayed_ammo,
            health: &self.health,
            armor: &self.armor,
            force: &self.force,
            weapon: &self.weapon,
            ammo: &self.ammo,
            health_value: &self.health_value,
            armor_value: &self.armor_value,
            force_value: &self.force_value,
            weapon_value: &self.weapon_value,
            ammo_value: &self.ammo_value,
            saber_style: self.values.and_then(|values| values.saber_style),
            weapon_alpha: self.weapon_alpha,
            weapon_row: self.weapon_select.shown.is_some(),
            vote_active: self.vote_active,
            team_vote_active: self.team_vote_active,
            crosshair_name: !self.crosshair_name.is_empty(),
            timer: !self.match_timer.is_empty(),
            warmup: !self.warmup_text.is_empty(),
            interrupted: self.interrupted,
        };
        let mut visible = [false; WIDGET_LIMIT];
        for (index, widget) in document.widgets.iter().take(WIDGET_LIMIT).enumerate() {
            visible[index] = visibility.hud
                && widget.visibility.evaluate(&data)
                && self.family.visible(widget.binding.as_deref());
        }
        let dpi_scale =
            crate::ui_scale::height_scale(viewport[1]).max(2.0 / 3.0) * user_scale.clamp(0.25, 2.0);
        let hero_scale = crate::ui_scale::height_scale(viewport[1]) * user_scale.clamp(0.25, 2.0);
        // Layout runs in logical pixels: the screen's size is its physical size over the scale.
        let logical_screen = [viewport[0] / dpi_scale, viewport[1] / dpi_scale];
        self.tree.clear();
        for (index, widget) in document.widgets.iter().take(WIDGET_LIMIT).enumerate() {
            let _ = self.tree.add(Widget {
                id: WidgetId(index as u32),
                parent: None,
                layout: LayoutKind::Anchored {
                    anchor: widget.anchor,
                    offset: Vec2::new(
                        widget.offset.x + widget.offset_fraction.x * logical_screen[0],
                        widget.offset.y + widget.offset_fraction.y * logical_screen[1],
                    ),
                },
                size: widget.size,
                visible: visible[index],
                focusable: false,
                scrollable: false,
                opacity: 1.0,
                z: widget.layer,
            });
        }
        let rectangles = LayoutEngine.layout(
            &self.tree,
            LayoutContext {
                viewport_physical: Vec2::new(viewport[0], viewport[1]),
                dpi_scale,
                safe_area: Insets::all(0.0),
            },
            &mut self.scratch,
        );
        self.draw_list.clear();
        self.tints.emit(&mut self.draw_list, viewport);
        self.rocket_lock.emit(&mut self.draw_list);
        // A pilot's crosshair is doubled, and is the vehicle's own picture when its
        // `.veh` names one that loaded.
        let crosshair_look = self.vehicle.crosshair_look(self.targeting.policy.look);
        self.crosshair_picture_drawn = visibility.crosshair
            && match self.vehicle.crosshair_picture() {
                Some(texture) => crosshair::emit_picture(
                    &mut self.draw_list,
                    texture,
                    crosshair_look,
                    self.targeting.center(viewport),
                    self.targeting.color,
                    viewport,
                ),
                None => crosshair::emit(
                    &mut self.draw_list,
                    &self.crosshair_pictures,
                    crosshair_look,
                    self.targeting.center(viewport),
                    self.targeting.color,
                    viewport,
                ),
            };
        let mut output = HudLayout::default();
        let low_health = self.values.is_some_and(|value| value.health <= 25);
        let low_ammo = self
            .values
            .is_some_and(|value| value.ammo.is_some_and(|ammo| ammo <= 5));
        let pulse = Tween::pulse(0.68, 1.0, time_ms, self.theme.motion.slow);
        // Bottom of what stands at the top right, which the kill feed goes under.
        let mut top_right = 0.0_f32;
        for (index, (widget, rect)) in document.widgets.iter().zip(rectangles).enumerate() {
            if !visible[index] {
                continue;
            }
            widgets::emit(
                &mut self.draw_list,
                self.theme,
                widget,
                *rect,
                &widgets::EmitContext {
                    data: &data,
                    family: self.family,
                    targeting: self.targeting.policy,
                    viewport,
                    dpi_scale,
                    hero_scale,
                    low_health,
                    low_ammo,
                    pulse,
                    crosshair_alpha: self.crosshair_alpha,
                    crosshair_teammate: self.crosshair_teammate,
                    lagometer: &self.lagometer,
                },
                &mut output,
            );
        }
        if visibility.hud {
            if visibility.status {
                self.vehicle
                    .emit(&mut self.draw_list, self.theme, viewport, user_scale);
            }
            self.speed.emit(&mut self.draw_list, self.theme, viewport);
            let selector = self
                .selector
                .filter(|s| !s.inventory || self.family.inventory);
            match selector {
                Some(view) if !view.inventory && self.force_wheel_bar => force_wheel::emit(
                    &mut self.draw_list,
                    view,
                    &self.force_wheel_icons,
                    self.flamethrower_shown,
                    viewport,
                    user_scale,
                ),
                view => selection::emit(
                    &mut self.draw_list,
                    view,
                    self.flamethrower_shown,
                    self.theme,
                    viewport,
                    user_scale,
                ),
            }
            top_right = top_right.max(self.emit_family(viewport));
            top_right = top_right.max(self.icons.emit(
                &mut self.draw_list,
                viewport,
                visibility,
                self.family.upper,
                self.weapon_select.shown.is_some(),
                self.weapon_alpha,
            ));
            let options = self.kill_feed.options();
            if options.fps || self.upper_right_timer {
                top_right = top_right.max(crate::console_overlay::upper_right_bottom(
                    viewport[1],
                    options.fps,
                    self.upper_right_timer,
                ));
            }
            self.kill_feed.emit(
                &mut self.draw_list,
                kill_feed::Area::below(viewport, hero_scale, dpi_scale, top_right, options),
                kill_feed::Art {
                    icons: &self.icons,
                    holocrons: &self.force_wheel_icons,
                },
                font,
                self.theme,
            );
            self.guides.emit(&mut self.draw_list, viewport);
        }
        // CG_DrawWeaponSelect does not follow cg_drawHud: only the row's own
        // conditions (`GpuState::sample_weapon_select`) decide.
        if let Some(shown) = &self.weapon_select.shown {
            crate::weapon_select::emit_icons(&mut self.draw_list, &self.icons, shown, viewport);
        }
        output
    }

    /// Settle the status values and meters on a fixed state, for the layout snapshots,
    /// with the weapon name showing as right after a weapon change.
    #[cfg(test)]
    pub(crate) fn preview_values(&mut self, values: ClientHudData, ratios: [f32; 4]) {
        self.format_values(values);
        self.weapon_shown_ms = Some(0);
        self.ratios = [ratios[0], ratios[1], ratios[2]].map(Tween::settled);
        self.ammo_ratio = Tween::settled(ratios[3]);
    }

    pub(crate) fn displayed_ratios(&self) -> [f32; 3] {
        self.displayed_ratios
    }

    /// Append HUD text: what retail drew with a game font goes to that font when
    /// `ui_gameFont` has it loaded ([`text_values::retail_font`]), the rest to
    /// `vertices` with `font`.
    pub(crate) fn append(
        &self,
        fonts: &mut crate::game_font::GameFonts,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        fonts.append_routed(
            &self.draw_list,
            |id| self.resolve_text(id),
            |id, _| text_values::retail_font(id),
            (vertices, font),
            viewport,
            crate::text::TextStyle::NEUTRAL,
        );
    }

    pub(crate) fn draw_list(&self) -> &DrawList {
        &self.draw_list
    }
}

#[cfg(test)]
mod tests {
    use super::{CLASSIC_LAYOUT, DEFAULT_LAYOUT};
    use sjk_ui::{Anchor, Dimension, HudLayoutDocument};

    /// Both layouts put the crosshair name's line top where stock's is:
    /// `CG_DrawCrosshairNames` draws it at y = 170 in the 480-line virtual
    /// screen, 157.5 px above the centre in the HUD's 1080-line frame.
    #[test]
    fn crosshair_name_line_top_matches_stock() {
        for layout in [DEFAULT_LAYOUT, CLASSIC_LAYOUT] {
            let document = HudLayoutDocument::from_json(layout).unwrap();
            let widget = document
                .widgets
                .iter()
                .find(|widget| widget.id == "crosshair_name")
                .unwrap();
            assert_eq!(widget.anchor, Anchor::Center);
            let Dimension::Px(height) = widget.size.height else {
                panic!("crosshair name height is not in px");
            };
            let top = 540.0 - height * 0.5 + widget.offset.y;
            assert!((top - 170.0 * 1_080.0 / 480.0).abs() < 1e-3, "{top}");
        }
    }
}
