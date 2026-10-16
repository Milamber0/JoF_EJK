//! The named HUD items `CG_DrawHUD` looks up, resolved once per load.
//!
//! OpenJK codemp `cg_draw.c` finds the `lefthud` and `righthud` menus by name
//! (`Menus_FindByName`, case-insensitive) and draws fixed item names from them
//! with `Menu_FindItemByName`, which returns the first match. An item's screen
//! rectangle is its `rect` offset by its menu's (`Item_SetScreenCoords`).
//! Pictures are referred to by index into [`Layout::pictures`].

use super::parse::{MenuDef, Window};

/// Tics per meter (`MAX_HUD_TICS`).
pub(crate) const TICS: usize = 4;

/// Which screen edge a menu keeps to when the screen is wider than 4:3.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Side {
    Left,
    Right,
}

/// One drawable piece: a rectangle in the 640x480 screen, the edge it keeps
/// to, an optional picture and its colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Piece {
    pub(crate) rect: [f32; 4],
    pub(crate) side: Side,
    pub(crate) picture: Option<u16>,
    pub(crate) color: [f32; 4],
}

/// `lefthud` or `righthud` as `CG_DrawHUD` uses it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MenuPieces {
    /// What `Menu_Paint` draws itself: the menu's and visible items'
    /// `WINDOW_STYLE_FILLED`/`WINDOW_STYLE_SHADER` backgrounds.
    pub(crate) painted: Vec<Piece>,
    pub(crate) scanline: Option<Piece>,
    pub(crate) frame: Option<Piece>,
}

/// Every item the retail status HUD draws, from the loaded menus.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Layout {
    pub(crate) left: Option<MenuPieces>,
    pub(crate) right: Option<MenuPieces>,
    pub(crate) health_tics: [Option<Piece>; TICS],
    pub(crate) armor_tics: [Option<Piece>; TICS],
    pub(crate) force_tics: [Option<Piece>; TICS],
    pub(crate) ammo_tics: [Option<Piece>; TICS],
    pub(crate) health_amount: Option<Piece>,
    pub(crate) armor_amount: Option<Piece>,
    pub(crate) force_amount: Option<Piece>,
    pub(crate) ammo_amount: Option<Piece>,
    pub(crate) ammo_infinite: Option<Piece>,
    pub(crate) score_line: Option<Piece>,
    /// `saberstyle_*` by `saber_styles_t` (1 fast ... 7 staff; 0 unused).
    pub(crate) saber_styles: [Option<Piece>; 8],
    /// Shader names, indexed by [`Piece::picture`].
    pub(crate) pictures: Vec<String>,
}

const HEALTH_TICS: [&str; TICS] = ["health_tic1", "health_tic2", "health_tic3", "health_tic4"];
const ARMOR_TICS: [&str; TICS] = ["armor_tic1", "armor_tic2", "armor_tic3", "armor_tic4"];
const FORCE_TICS: [&str; TICS] = ["force_tic1", "force_tic2", "force_tic3", "force_tic4"];
const AMMO_TICS: [&str; TICS] = ["ammo_tic1", "ammo_tic2", "ammo_tic3", "ammo_tic4"];
/// `saber_styles_t` order. `CG_DrawSaberStyle` draws only fast, medium and
/// strong; the others are EternalJK/JoF additions some HUD packs provide.
const SABER_STYLES: [&str; 8] = [
    "",
    "saberstyle_fast",
    "saberstyle_medium",
    "saberstyle_strong",
    "saberstyle_desann",
    "saberstyle_tavion",
    "saberstyle_dual",
    "saberstyle_staff",
];

impl Layout {
    /// Resolve the status HUD from `menus` (later definitions of a menu name
    /// do not replace the first, as `Menus_FindByName` returns the first).
    pub(crate) fn resolve(menus: &[MenuDef]) -> Self {
        let mut layout = Self::default();
        let find = |name: &str| {
            menus
                .iter()
                .find(|menu| menu.window.name.eq_ignore_ascii_case(name))
        };
        if let Some(menu) = find("lefthud") {
            layout.left = Some(layout.side(menu));
            layout.health_tics = HEALTH_TICS.map(|name| layout.item(menu, name));
            layout.armor_tics = ARMOR_TICS.map(|name| layout.item(menu, name));
            layout.health_amount = layout.item(menu, "healthamount");
            layout.armor_amount = layout.item(menu, "armoramount");
        }
        if let Some(menu) = find("righthud") {
            layout.right = Some(layout.side(menu));
            layout.force_tics = FORCE_TICS.map(|name| layout.item(menu, name));
            layout.ammo_tics = AMMO_TICS.map(|name| layout.item(menu, name));
            layout.force_amount = layout.item(menu, "forceamount");
            layout.ammo_amount = layout.item(menu, "ammoamount");
            layout.ammo_infinite = layout.item(menu, "ammoinfinite");
            layout.score_line = layout.item(menu, "score_line");
            layout.saber_styles = SABER_STYLES.map(|name| {
                (!name.is_empty())
                    .then(|| layout.item(menu, name))
                    .flatten()
            });
        }
        layout
    }

    /// Visit every resolved piece once.
    pub(crate) fn for_each_piece(&self, mut visit: impl FnMut(&Piece)) {
        for side in self.left.iter().chain(&self.right) {
            side.painted.iter().for_each(&mut visit);
            side.scanline.iter().chain(&side.frame).for_each(&mut visit);
        }
        let singles = [
            &self.health_amount,
            &self.armor_amount,
            &self.force_amount,
            &self.ammo_amount,
            &self.ammo_infinite,
            &self.score_line,
        ];
        singles.into_iter().flatten().for_each(&mut visit);
        self.health_tics
            .iter()
            .chain(&self.armor_tics)
            .chain(&self.force_tics)
            .chain(&self.ammo_tics)
            .chain(&self.saber_styles)
            .flatten()
            .for_each(visit);
    }

    /// Whether the files describe a HUD at all.
    pub(crate) fn is_usable(&self) -> bool {
        self.left.is_some() || self.right.is_some()
    }

    fn side(&mut self, menu: &MenuDef) -> MenuPieces {
        let mut side = MenuPieces {
            painted: Vec::new(),
            scanline: self.item(menu, "scanline"),
            frame: self.item(menu, "frame"),
        };
        let edge = edge(&menu.window);
        if let Some(piece) = self.painted(&menu.window, [0.0; 2], edge) {
            side.painted.push(piece);
        }
        let origin = [menu.window.rect[0], menu.window.rect[1]];
        for item in menu.items.iter().filter(|item| item.visible) {
            if let Some(piece) = self.painted(item, origin, edge) {
                side.painted.push(piece);
            }
        }
        side
    }

    /// `Window_Paint`'s background for a window with a background style.
    fn painted(&mut self, window: &Window, origin: [f32; 2], side: Side) -> Option<Piece> {
        let rect = absolute(window, origin);
        match window.style {
            // WINDOW_STYLE_FILLED: the background tinted by backcolor, or a
            // plain backcolor box.
            1 => Some(Piece {
                rect,
                side,
                picture: window.background.as_deref().map(|name| self.picture(name)),
                color: window.back_color,
            }),
            // WINDOW_STYLE_SHADER, tinted by forecolor only when one was set.
            3 => Some(Piece {
                rect,
                side,
                picture: Some(self.picture(window.background.as_deref()?)),
                color: if window.fore_color_set {
                    window.fore_color
                } else {
                    [1.0; 4]
                },
            }),
            _ => None,
        }
    }

    /// The first item of `menu` named `name`, with its picture registered.
    fn item(&mut self, menu: &MenuDef, name: &str) -> Option<Piece> {
        let item = menu
            .items
            .iter()
            .find(|item| item.name.eq_ignore_ascii_case(name))?;
        let origin = [menu.window.rect[0], menu.window.rect[1]];
        Some(Piece {
            rect: absolute(item, origin),
            side: edge(&menu.window),
            picture: item.background.as_deref().map(|name| self.picture(name)),
            color: item.fore_color,
        })
    }

    fn picture(&mut self, name: &str) -> u16 {
        let index = self
            .pictures
            .iter()
            .position(|known| known.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| {
                self.pictures.push(name.to_owned());
                self.pictures.len() - 1
            });
        index as u16
    }
}

fn absolute(window: &Window, origin: [f32; 2]) -> [f32; 4] {
    let [x, y, w, h] = window.rect;
    [origin[0] + x, origin[1] + y, w, h]
}

/// The edge a menu keeps to: `righthud` the right one, every other menu the
/// left, as EternalJK's widescreen correction places `CG_DrawHUD`'s items
/// (left-HUD items scale from the left edge, right-HUD items from the right,
/// wherever a HUD pack puts them).
fn edge(window: &Window) -> Side {
    if window.name.eq_ignore_ascii_case("righthud") {
        Side::Right
    } else {
        Side::Left
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse::parse;
    use super::*;

    const FILE: &str = r#"{
menuDef { name "LeftHud" rect 0 368 112 112
  itemDef { name "frame" background "gfx/hud/hudleft" rect 0 0 112 112 }
  itemDef { name health_tic1 background "gfx/hud/health_tic_1" rect 20 24 28 28 }
  itemDef { name healthamount forecolor .835 .015 .015 1 rect 59 98 6 12 }
  itemDef { name glow style 3 visible 1 background "gfx/hud/glow" rect 1 2 3 4 }
  itemDef { name hidden style 3 background "gfx/hud/glow" rect 1 2 3 4 }
}
menuDef { name "righthud" rect 640 368 -112 112
  itemDef { name "frame" background "gfx/hud/hudleft" rect 0 0 -112 112 }
  itemDef { name ammo_tic1 background "gfx/hud/ammo_tic_1" rect -48 25 28 28 }
  itemDef { name saberstyle_staff background "gfx/hud/saber_staff" rect -70 43 26 26 }
}
menuDef { name "lefthud" rect 0 0 10 10 }
}"#;

    #[test]
    fn resolves_named_items_to_screen_rectangles() {
        let layout = Layout::resolve(&parse(FILE).menus);
        assert!(layout.is_usable());
        let left = layout.left.as_ref().unwrap();
        let frame = left.frame.unwrap();
        assert_eq!(frame.rect, [0.0, 368.0, 112.0, 112.0]);
        assert_eq!(frame.side, Side::Left);
        assert_eq!(
            layout.health_tics[0].unwrap().rect,
            [20.0, 392.0, 28.0, 28.0]
        );
        assert!(layout.health_tics[1].is_none());
        let amount = layout.health_amount.unwrap();
        assert_eq!(amount.color, [0.835, 0.015, 0.015, 1.0]);
        assert!(amount.picture.is_none());
        // Only the visible styled item is painted by Menu_Paint.
        assert_eq!(left.painted.len(), 1);
        assert_eq!(left.painted[0].rect, [1.0, 370.0, 3.0, 4.0]);
        let right = layout.right.as_ref().unwrap();
        assert_eq!(right.frame.unwrap().rect, [640.0, 368.0, -112.0, 112.0]);
        assert_eq!(right.frame.unwrap().side, Side::Right);
        assert_eq!(
            layout.ammo_tics[0].unwrap().rect,
            [592.0, 393.0, 28.0, 28.0]
        );
        assert!(layout.saber_styles[7].is_some() && layout.saber_styles[1].is_none());
        // One picture per distinct shader name.
        assert_eq!(
            layout.pictures,
            [
                "gfx/hud/hudleft",
                "gfx/hud/glow",
                "gfx/hud/health_tic_1",
                "gfx/hud/ammo_tic_1",
                "gfx/hud/saber_staff"
            ]
        );
    }

    #[test]
    fn files_without_hud_menus_are_unusable() {
        assert!(!Layout::resolve(&parse("menuDef { name \"mainhud\" }").menus).is_usable());
    }
}
