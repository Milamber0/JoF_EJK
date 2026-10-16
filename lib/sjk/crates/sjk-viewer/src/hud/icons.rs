//! Retained HUD icon policy and submissions, using the existing UI texture atlas.
use super::*;
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign, TextOverflow, TextureId};
#[path = "icon_assets.rs"]
pub(crate) mod assets;

/// Register only the implemented flat-icon path and explicitly partial model selector.
pub(crate) fn register(cvars: &mut sjk_shell::CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    for (name, default, help) in [
        (
            "cg_drawIcons",
            1,
            "Status weapon, ammo and carried-flag icons",
        ),
        (
            "cg_draw3DIcons",
            0,
            "Flag slot: 0 flat; 1 model variant currently unavailable",
        ),
        (
            "cg_drawPowerUpIcons",
            1,
            "Timed powerup icons and countdowns",
        ),
    ] {
        cvars.register(sjk_shell::CvarDefinition::new(
            name,
            default as i64,
            sjk_shell::CvarFlags::ARCHIVE,
            help,
        ))?;
    }
    Ok(())
}

/// Install map-specific image handles without a second texture loader or render pass.
pub(crate) fn install(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    format: wgpu::TextureFormat,
    vfs: &sjk_vfs::VirtualFileSystem,
    shaders: &sjk_shader::ShaderCatalog,
) -> (ui_renderer::ShapeRenderer, HudOverlay) {
    let renderer =
        ui_renderer::ShapeRenderer::new(device, queue, format, crate::DepthTarget::FORMAT);
    let mut hud = HudOverlay::new();
    hud.icons.handles = assets::load(vfs, shaders, |id, rgba| {
        renderer.upload_icon(queue, id, rgba)
    });
    hud.weapon_select = crate::weapon_select::State::load(vfs);
    hud.force_wheel_icons = super::force_wheel::load(vfs, shaders, |id, rgba| {
        renderer.upload_icon(queue, id, rgba)
    });
    hud.holocron_icons = crate::holocrons::icons::load(vfs, shaders, |id, rgba| {
        renderer.upload_icon(queue, id, rgba)
    });
    hud.crosshair_pictures = super::crosshair::load(vfs, shaders, |id, rgba| {
        renderer.upload_icon(queue, id, rgba)
    });
    hud.rocket_lock
        .set_pictures(super::rocket_lock::load(vfs, shaders, |id, rgba| {
            renderer.upload_icon(queue, id, rgba)
        }));
    (renderer, hud)
}

/// Sampled options and scalar snapshot data; all texture handles are map-lifetime values.
pub(crate) struct Icons {
    handles: [Option<TextureId>; assets::COUNT],
    enabled: bool,
    models: bool,
    powers: bool,
    weapon: u8,
    alive: bool,
    seconds: [i32; 16],
    cty: bool,
    warned_model: bool,
}

impl Icons {
    /// The weapon selection row's icon of `weapon` (`weaponIcon` in JoF EJK's
    /// `cg_weapons.c`): its `_na` icon when `empty`, else the staff or dual saber
    /// icon for those `saber_style`s, else its own; a missing variant falls back
    /// to the weapon's own icon.
    pub(crate) fn weapon_select(
        &self,
        weapon: u8,
        empty: bool,
        saber_style: u8,
    ) -> Option<TextureId> {
        let own = *assets::WEAPONS.get(usize::from(weapon))?;
        let variant = if empty {
            assets::EMPTY_WEAPONS + usize::from(weapon)
        } else if saber_style == crate::weapon_select::SS_STAFF {
            assets::SABER_STAFF
        } else if saber_style == crate::weapon_select::SS_DUAL {
            assets::SABER_DUAL
        } else {
            own
        };
        self.handles[variant].or(self.handles[own])
    }

    /// The loaded icon of timed powerup `power` (`PW_REDFLAG` is 4), if any.
    pub(crate) fn powerup(&self, power: usize) -> Option<TextureId> {
        let item = *assets::POWERS.get(power)?;
        (item != 0).then(|| self.handles[item]).flatten()
    }

    /// The loaded picture of team logo `logo`, if any.
    pub(crate) fn team_logo(&self, logo: super::team_logo::Logo) -> Option<TextureId> {
        self.handles[assets::TEAM_LOGO_FIRST + logo as usize]
    }

    /// The loaded cause-of-death picture of `means` (`meansOfDeath_t`): an icon
    /// pack's `hud/mod/*`, else the picture of the weapon that deals it.
    pub(crate) fn means(&self, means: u8) -> Option<TextureId> {
        let means = usize::from(means);
        (means < assets::MEANS_COUNT)
            .then(|| self.handles[assets::MEANS_FIRST + means])
            .flatten()
    }

    /// Icons with only `weapons`' own pictures, as `(weapon, texture)`, for the
    /// off-screen snapshots.
    #[cfg(test)]
    pub(crate) fn with_weapons(weapons: &[(u8, TextureId)]) -> Self {
        let mut icons = Self::default();
        for (weapon, texture) in weapons {
            if let Some(slot) = assets::WEAPONS.get(usize::from(*weapon)) {
                icons.handles[*slot] = Some(*texture);
            }
        }
        icons
    }

    /// Icons with only `means`' cause-of-death pictures, as `(MOD_*, texture)`.
    #[cfg(test)]
    pub(crate) fn with_means(means: &[(u8, TextureId)]) -> Self {
        let mut icons = Self::default();
        for (means, texture) in means {
            icons.handles[assets::MEANS_FIRST + usize::from(*means)] = Some(*texture);
        }
        icons
    }
}

impl Default for Icons {
    fn default() -> Self {
        Self {
            handles: [None; assets::COUNT],
            enabled: true,
            models: false,
            powers: true,
            weapon: 0,
            alive: false,
            seconds: [-1; 16],
            cty: false,
            warned_model: false,
        }
    }
}

impl Icons {
    /// Use the same predicted weapon selection as the existing status text.
    pub(super) fn weapon(&mut self, weapon: u8) {
        {
            self.weapon = weapon;
        }
    }

    /// Cache settings once per HUD update; format no strings and perform no asset lookup.
    pub(super) fn update(
        &mut self,
        snapshot: &sjk_protocol::Snapshot,
        game: &GameState,
        console: Option<&ViewerConsole>,
    ) {
        self.enabled = family::integer(console, "cg_drawicons", 1) != 0;
        self.models = family::integer(console, "cg_draw3dicons", 0) != 0;
        self.powers = family::integer(console, "cg_drawpowerupicons", 1) != 0;
        if self.models && !self.warned_model {
            eprintln!("3D HUD flag icons unavailable; use cg_draw3DIcons 0 for flat icons");
            self.warned_model = true;
        }

        self.weapon = snapshot.player.weapon();
        self.alive = snapshot.player.health() > 0 && snapshot.player.movement_type() != 4;
        self.cty = game
            .config_string(0)
            .and_then(|s| sjk_client::LegacyClientInfo::new(s).integer("g_gametype"))
            == Some(8);
        for (seconds, end) in self.seconds.iter_mut().zip(snapshot.player.powerups) {
            let remaining = i64::from(end) - i64::from(snapshot.server_time);
            *seconds = if remaining > 0 {
                (remaining / 1000) as i32
            } else {
                -1
            };
        }
    }

    fn quad(&self, list: &mut DrawList, index: usize, rect: Rect, color: Color) -> bool {
        let Some(texture) = self.handles.get(index).copied().flatten() else {
            return false;
        };
        let _ = list.push(DrawCommand::TexturedQuad {
            rect,
            texture,
            color,
        });
        true
    }

    /// Emit the status slots and the independently gated powerup column; returns
    /// the bottom of that column at the top right, or zero when it is empty.
    pub(super) fn emit(
        &self,
        list: &mut DrawList,
        viewport: [f32; 2],
        visibility: HudVisibility,
        upper: bool,
        weapon_row: bool,
        weapon_alpha: f32,
    ) -> f32 {
        let [w, h] = viewport;
        let s = crate::ui_scale::height_scale(h);
        let white = Color::new(1.0, 1.0, 1.0, 1.0);
        // Retail's weapon selection row takes the bottom centre while it shows; the
        // held weapon's icon would sit behind its selected icon.
        if self.enabled && visibility.status && self.alive && !weapon_row {
            let item = assets::WEAPONS
                .get(self.weapon as usize)
                .copied()
                .unwrap_or(0);
            // The held weapon's icon shows after a change and fades like its name
            // (no ammo picture: EternalJK draws neither at the bottom centre).
            if weapon_alpha > 0.0 {
                self.quad(
                    list,
                    item,
                    // Beside the ammo count (where the ammo picture was), not under it.
                    Rect::new(w / 2.0 + 48.0 * s, h - 88.0 * s, 64.0 * s, 64.0 * s),
                    Color::new(1.0, 1.0, 1.0, weapon_alpha),
                );
            }
        }
        if self.enabled && visibility.status && self.alive && !self.models {
            for power in 4..=6 {
                if self.seconds[power] >= 0 {
                    self.quad(
                        list,
                        assets::POWERS[power],
                        Rect::new(420.0 * s, h - 128.0 * s, 64.0 * s, 64.0 * s),
                        white,
                    );
                }
            }
        }
        if !self.powers || !upper {
            return 0.0;
        }
        let mut bottom = 0.0;
        let mut y = 200.0 * s;
        for (power, seconds) in self.seconds.iter().copied().enumerate() {
            if seconds < 0 || assets::POWERS[power] == 0 {
                continue;
            }
            let index = if self.cty && matches!(power, 4 | 5) {
                90 + power
            } else {
                assets::POWERS[power]
            };
            if self.quad(
                list,
                index,
                Rect::new(w - 100.0 * s, y, 64.0 * s, 64.0 * s),
                white,
            ) {
                bottom = y + 64.0 * s;
                if !matches!(power, 4 | 5) && seconds < 999 {
                    bottom = y + 84.0 * s;
                    let _ = list.push(DrawCommand::Text {
                        rect: Rect::new(w - 100.0 * s, y + 56.0 * s, 64.0 * s, 28.0 * s),
                        text: TextId(400 + power as u32),
                        size: 22.0 * s,
                        color: white,
                        align: TextAlign::Center,
                        overflow: TextOverflow::Clip,
                        weight: FontWeight::Semibold,
                        letter_spacing: 0.0,
                    });
                }
                y += 86.0 * s;
            }
        }
        bottom
    }

    /// Borrow a precomputed decimal string; countdown updates allocate and format nothing.
    pub(super) fn text(&self, power: usize) -> &str {
        let value = self.seconds[power].clamp(0, 998) as usize;
        let start = if value >= 100 {
            0
        } else if value >= 10 {
            1
        } else {
            2
        };
        std::str::from_utf8(&NUMBERS[value][start..]).unwrap_or("")
    }
}

const NUMBERS: [[u8; 3]; 999] = {
    let mut values = [[b'0'; 3]; 999];
    let mut i = 0;
    while i < values.len() {
        values[i] = [
            b'0' + (i / 100) as u8,
            b'0' + ((i / 10) % 10) as u8,
            b'0' + (i % 10) as u8,
        ];
        i += 1;
    }
    values
};
