//! The HUD picker: Enter or a click on the HUD settings' "HUD" row opens a
//! list of every HUD that can be used (see [`crate::menu_hud::choices`])
//! beside a picture of the highlighted one, drawn for a sample player (see
//! [`crate::menu_hud::preview`]). Arrows, the wheel or the pointer move the
//! highlight; Enter or a click uses the HUD and closes; Escape (or the ESC
//! hint) closes without changing it. Left and right on the row itself step
//! through the same list without opening it.
//!
//! The list needs the mounted files and the previews the shader catalogue
//! too, which the menu does not hold: [`SettingsMenu::service_hud_picker`]
//! is given both every frame the settings are up. Previews render on a
//! worker, one at a time, and stay cached until the picker closes.

use super::*;
use crate::menu::art::{ArtPiece, ArtSet};
use crate::menu::classic::layout::Placement;
use crate::menu::levelshot::LevelshotImage;
use crate::menu_hud::choices::{self, HudChoice};
use crate::menu_hud::preview::{self, PreviewText};
use crate::menu_hud::{FILES_CVAR, HudStyle, PACK_CVAR};
use crate::menu_widgets::BACK_TOKEN;
use sjk_ui::{Color, DrawCommand, FontWeight, InputEvent, TextAlign, UiEventKind};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

/// Wheel target over the list.
const LIST_SCROLL_TOKEN: u16 = 913;
/// Rows the list shows at once.
const LIST_ROWS: usize = 15;
/// Picker geometry on the 640x480 canvas: the box, its title band, the
/// list's first row and row height, and the preview (16:9).
const PANEL: [f32; 4] = [20.0, 50.0, 600.0, 380.0];
const TITLE: [f32; 4] = [20.0, 52.0, 600.0, 22.0];
const LIST: [f32; 2] = [32.0, 84.0];
const LIST_WIDTH: f32 = 170.0;
const ROW_HEIGHT: f32 = 20.0;
const PREVIEW: [f32; 4] = [214.0, 84.0, 394.0, 394.0 * 9.0 / 16.0];
const HINT_Y: f32 = 404.0;

/// A rendered preview, ready to upload.
struct Ready {
    image: LevelshotImage,
    texts: Vec<PreviewText>,
}

/// The cvars that select a HUD, as last read.
#[derive(Clone, Debug, Default, PartialEq)]
struct Selected {
    style: Option<HudStyle>,
    files: String,
    pack: String,
}

#[derive(Default)]
pub(super) struct HudPicker {
    open: bool,
    /// The cvars that select the HUD in use.
    selected: Selected,
    choices: Vec<HudChoice>,
    /// The list is out of date (the settings or the picker just opened).
    stale: bool,
    /// Highlighted choice and the first one the list shows.
    highlight: usize,
    first: usize,
    /// Previews made since the picker opened, by choice.
    previews: Vec<Option<Arc<Ready>>>,
    /// Choices whose preview could not be made.
    failed: Vec<bool>,
    /// The preview being rendered, and for which choice.
    rendering: Option<(usize, Receiver<Option<Ready>>)>,
    /// The choice whose preview is in the texture.
    shown: Option<usize>,
}

impl HudPicker {
    pub(super) fn is_open(&self) -> bool {
        self.open
    }

    /// Read the list again when the files are next available.
    pub(super) fn mark_stale(&mut self) {
        self.stale = true;
    }

    /// Take the cvars that select the HUD.
    pub(super) fn read(&mut self, console: &ViewerConsole) {
        self.selected = Selected {
            style: Some(HudStyle::read(Some(console))),
            files: console
                .text_value(FILES_CVAR)
                .unwrap_or_default()
                .to_owned(),
            pack: console.text_value(PACK_CVAR).unwrap_or_default().to_owned(),
        };
    }

    /// Index of the choice in use.
    fn current(&self) -> Option<usize> {
        choices::current(
            &self.choices,
            self.selected.style?,
            &self.selected.files,
            &self.selected.pack,
        )
    }

    /// What the HUD row shows: the HUD in use, by name once the list is
    /// read, else by its style.
    pub(super) fn label(&self) -> String {
        if let Some(index) = self.current() {
            return self.choices[index].label.clone();
        }
        match self.selected.style {
            Some(HudStyle::Classic) => "SJK classic",
            Some(HudStyle::Radial) => "SJK radial",
            _ => "Game HUD",
        }
        .to_owned()
    }

    fn open(&mut self) {
        self.open = true;
        self.stale = true;
        self.clear_previews();
    }

    fn close(&mut self) {
        self.open = false;
        self.clear_previews();
    }

    fn clear_previews(&mut self) {
        self.previews.clear();
        self.failed.clear();
        self.rendering = None;
        self.shown = None;
    }

    fn highlight(&mut self, index: usize) {
        if self.choices.is_empty() {
            return;
        }
        self.highlight = index.min(self.choices.len() - 1);
        if self.highlight < self.first {
            self.first = self.highlight;
        } else if self.highlight >= self.first + LIST_ROWS {
            self.first = self.highlight + 1 - LIST_ROWS;
        }
    }

    fn scroll(&mut self, rows: isize) {
        let last = self.choices.len().saturating_sub(LIST_ROWS);
        self.first = self.first.saturating_add_signed(rows).min(last);
    }
}

impl SettingsMenu {
    /// Open the picker on the HUD in use.
    pub(super) fn open_hud_picker(&mut self, console: &ViewerConsole) {
        self.hud.read(console);
        self.hud.open();
        if let Some(current) = self.hud.current() {
            self.hud.highlight(current);
        }
    }

    /// Use choice `index`: write its cvars and show it on the HUD row.
    fn use_hud(&mut self, console: &mut ViewerConsole, index: usize) {
        if let Some(choice) = self.hud.choices.get(index) {
            choice.apply(console);
        }
        self.refresh(console);
    }

    /// Left or right on the HUD row: the previous or next HUD of the list.
    pub(super) fn step_hud(&mut self, console: &mut ViewerConsole, direction: i32) {
        let count = self.hud.choices.len();
        if count == 0 {
            return;
        }
        let next = match self.hud.current() {
            Some(index) => (index as i32 + direction).rem_euclid(count as i32) as usize,
            None => 0,
        };
        self.use_hud(console, next);
    }

    /// A key while the picker is open.
    pub(super) fn hud_picker_key(
        &mut self,
        key: KeyCode,
        repeat: bool,
        console: &mut ViewerConsole,
    ) {
        let count = self.hud.choices.len();
        let highlight = self.hud.highlight;
        match key {
            // A held Enter or Escape that opened the picker must not act on it.
            KeyCode::Escape | KeyCode::Backspace if !repeat => self.hud.close(),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space if !repeat => {
                if highlight < count {
                    self.use_hud(console, highlight);
                }
                self.hud.close();
            }
            KeyCode::ArrowUp | KeyCode::KeyW => {
                self.hud
                    .highlight(highlight.checked_sub(1).unwrap_or(count.saturating_sub(1)));
            }
            KeyCode::ArrowDown | KeyCode::KeyS => {
                self.hud.highlight(if highlight + 1 >= count {
                    0
                } else {
                    highlight + 1
                });
            }
            KeyCode::PageUp => self.hud.highlight(highlight.saturating_sub(LIST_ROWS)),
            KeyCode::PageDown => self.hud.highlight(highlight + LIST_ROWS),
            KeyCode::Home => self.hud.highlight(0),
            KeyCode::End => self.hud.highlight(count.saturating_sub(1)),
            _ => {}
        }
    }

    /// Pointer while the picker is open: hover highlights (and previews), a
    /// click uses the HUD, the wheel scrolls the list, the ESC hint closes.
    pub(super) fn hud_picker_pointer(&mut self, event: InputEvent, console: &mut ViewerConsole) {
        let Some(event) = self.ui.pointer(event) else {
            return;
        };
        let Some(token) = event.token else {
            return;
        };
        let slot = usize::from(token);
        let on_row = slot < LIST_ROWS && self.hud.first + slot < self.hud.choices.len();
        match event.kind {
            UiEventKind::Wheel => {
                let notches = event.delta.map_or(0, |delta| -delta.y.signum() as isize);
                self.hud.scroll(notches);
            }
            UiEventKind::HoverEnter | UiEventKind::Hover if on_row => {
                self.hud.highlight(self.hud.first + slot);
            }
            UiEventKind::Activate if token == BACK_TOKEN => self.hud.close(),
            UiEventKind::Activate if on_row => {
                self.use_hud(console, self.hud.first + slot);
                self.hud.close();
            }
            _ => {}
        }
    }

    /// Upload the highlighted HUD's preview again, into another world's
    /// texture.
    pub(crate) fn forget_hud_preview_upload(&mut self) {
        self.hud.shown = None;
    }

    /// Per frame while the settings are up: read the HUD list when it is
    /// stale, collect a finished preview, start the highlighted one's and
    /// hand `upload` the preview to show when it changes.
    pub(crate) fn service_hud_picker(
        &mut self,
        vfs: Option<&Arc<sjk_vfs::VirtualFileSystem>>,
        shaders: &sjk_shader::ShaderCatalog,
        mut upload: impl FnMut(&LevelshotImage),
    ) {
        let Some(vfs) = vfs else {
            return;
        };
        let picker = &mut self.hud;
        if picker.stale {
            picker.stale = false;
            picker.choices = choices::list(vfs);
            picker.clear_previews();
            if picker.open
                && let Some(current) = picker.current()
            {
                picker.highlight(current);
            }
            self.refresh_hud_row();
        }
        let picker = &mut self.hud;
        if !picker.open {
            return;
        }
        let count = picker.choices.len();
        picker.previews.resize_with(count, || None);
        picker.failed.resize(count, false);
        let finished = match &picker.rendering {
            Some((index, receiver)) => match receiver.try_recv() {
                Ok(ready) => Some((*index, ready)),
                Err(mpsc::TryRecvError::Disconnected) => Some((*index, None)),
                Err(mpsc::TryRecvError::Empty) => None,
            },
            None => None,
        };
        if let Some((index, ready)) = finished {
            picker.rendering = None;
            match ready {
                Some(ready) => picker.previews[index] = Some(Arc::new(ready)),
                None => picker.failed[index] = true,
            }
        }
        let wanted = picker.highlight;
        let Some(choice) = picker.choices.get(wanted) else {
            return;
        };
        if let Some(ready) = &picker.previews[wanted] {
            if picker.shown != Some(wanted) {
                upload(&ready.image);
                picker.shown = Some(wanted);
            }
            return;
        }
        if picker.rendering.is_some() || picker.failed[wanted] || !choice.has_preview() {
            return;
        }
        let Some(plan) = preview::plan(vfs, shaders, choice) else {
            picker.failed[wanted] = true;
            return;
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("HUD preview".to_owned())
            .spawn(move || {
                let preview = preview::render(&plan);
                let _ = sender.send(Some(Ready {
                    image: LevelshotImage::from_rgba(preview.image),
                    texts: preview.texts,
                }));
            });
        match spawned {
            Ok(_) => picker.rendering = Some((wanted, receiver)),
            Err(_) => picker.failed[wanted] = true,
        }
    }

    /// Show the HUD in use on the HUD row.
    fn refresh_hud_row(&mut self) {
        let label = self.hud.label();
        for (row, setting) in self.rows().iter().enumerate() {
            if matches!(setting.kind, ValueKind::HudPicker)
                && let Some(value) = self.values.get_mut(row)
            {
                value.clone_from(&label);
            }
        }
    }

    /// Draw the open picker over the SJK UI's Settings (the classic+ panels
    /// draw it themselves, with their art), in the theme's colours.
    pub(crate) fn append_hud_overlay(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        reveal: f32,
    ) {
        self.sjk_controls = None;
        self.append_hud_picker(vertices, font, viewport, reveal, None);
    }

    /// Draw the open picker over the screen: retail colours and art with
    /// `art` (the classic menus), the theme's otherwise.
    pub(super) fn append_hud_picker(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        reveal: f32,
        art: Option<ArtSet>,
    ) {
        let place = Placement::new(viewport);
        let s = place.scale;
        let palette = Palette::new(art, self.ui.theme());
        self.ui.begin_transparent(viewport);
        self.ui.push_opacity(reveal);
        let [width, height] = viewport;
        self.fill(Rect::new(0.0, 0.0, width, height), palette.scrim);
        let panel = place.rect(PANEL);
        self.fill(panel, palette.panel);
        self.outline(panel, palette.border, 1.0 * s);
        let title = place.rect(TITLE);
        if art.is_some_and(|art| art.has(ArtPiece::BlendBox)) {
            crate::menu::classic::view::art(&mut self.ui, ArtPiece::BlendBox, title);
        }
        self.line(
            "HUD",
            title,
            15.0 * s,
            palette.title,
            FontWeight::Semibold,
            TextAlign::Center,
        );
        self.hud_list(&place, &palette);
        self.hud_preview(&place, &palette);
        let hint = place.rect([PANEL[0], HINT_Y, PANEL[2], 14.0]);
        self.line(
            "UP / DOWN choose     ENTER use     ESC back",
            hint,
            11.0 * s,
            palette.hint,
            FontWeight::Regular,
            TextAlign::Center,
        );
        self.ui.hit_region(BACK_TOKEN, hint);
        self.ui.pop_opacity();
        let focus = self.hud.highlight.saturating_sub(self.hud.first);
        self.ui.finish(focus as u16);
        self.ui.append_text(vertices, font, viewport);
    }

    fn hud_list(&mut self, place: &Placement, palette: &Palette) {
        let s = place.scale;
        let picker = &self.hud;
        let (first, highlight, current) = (picker.first, picker.highlight, picker.current());
        let count = picker.choices.len();
        let list = place.rect([LIST[0], LIST[1], LIST_WIDTH, ROW_HEIGHT * LIST_ROWS as f32]);
        self.ui.scroll_region(LIST_SCROLL_TOKEN, list);
        if count == 0 {
            let row = place.rect([LIST[0], LIST[1], LIST_WIDTH, ROW_HEIGHT]);
            self.line(
                "Reading the HUDs...",
                row,
                12.0 * s,
                palette.text,
                FontWeight::Regular,
                TextAlign::Start,
            );
            return;
        }
        for slot in 0..LIST_ROWS.min(count - first) {
            let index = first + slot;
            let row = place.rect([
                LIST[0],
                LIST[1] + slot as f32 * ROW_HEIGHT,
                LIST_WIDTH,
                ROW_HEIGHT,
            ]);
            let lit = index == highlight;
            if lit {
                if palette.art.is_some_and(|art| art.has(ArtPiece::BlendBox)) {
                    crate::menu::classic::view::art(&mut self.ui, ArtPiece::BlendBox, row);
                } else {
                    self.fill(row, palette.highlight);
                }
            }
            let label = self.hud.choices[index].label.clone();
            let text = Rect::new(row.x + 8.0 * s, row.y, row.width - 26.0 * s, row.height);
            self.line(
                &label,
                text,
                13.0 * s,
                if lit { palette.focus } else { palette.text },
                if lit {
                    FontWeight::Semibold
                } else {
                    FontWeight::Regular
                },
                TextAlign::Start,
            );
            if Some(index) == current {
                let mark = Rect::new(row.right() - 18.0 * s, row.y, 14.0 * s, row.height);
                self.line(
                    "\u{2022}",
                    mark,
                    13.0 * s,
                    palette.in_use,
                    FontWeight::Semibold,
                    TextAlign::Center,
                );
            }
            self.ui.hit_region(slot as u16, row);
        }
        if count > LIST_ROWS {
            let track = Rect::new(list.right() + 2.0 * s, list.y, 2.0 * s, list.height);
            self.ui.list_scroll_mark(track, first, LIST_ROWS, count, s);
        }
    }

    fn hud_preview(&mut self, place: &Placement, palette: &Palette) {
        let s = place.scale;
        let frame = place.rect(PREVIEW);
        self.fill(frame, Color::new(0.0, 0.0, 0.0, 0.85));
        let picker = &self.hud;
        let Some(choice) = picker.choices.get(picker.highlight).cloned() else {
            self.outline(frame, palette.border, 1.0 * s);
            return;
        };
        let index = picker.highlight;
        let ready = picker.previews.get(index).cloned().flatten();
        let shown = ready.filter(|_| picker.shown == Some(index));
        let failed = picker.failed.get(index).copied().unwrap_or(false);
        let note = match (&shown, choice.has_preview(), failed) {
            (Some(ready), _, _) => {
                let _ = self.ui.draw_list_mut().push(DrawCommand::TexturedQuad {
                    rect: frame,
                    texture: crate::ui_renderer::HUD_PREVIEW_TEXTURE,
                    color: Color::new(1.0, 1.0, 1.0, 1.0),
                });
                // The HUD's text in the menu font, where the HUD puts it.
                let zoom = frame.width / preview::SIZE[0] as f32;
                for run in &ready.texts {
                    let size = run.size * zoom;
                    let [r, g, b, a] = run.color;
                    let (x, align) = if run.centred {
                        (frame.x + run.x * zoom - frame.width, TextAlign::Center)
                    } else {
                        (frame.x + run.x * zoom, TextAlign::Start)
                    };
                    let width = if run.centred {
                        frame.width * 2.0
                    } else {
                        frame.right() - x
                    };
                    self.line(
                        &run.text,
                        Rect::new(x, frame.y + run.y * zoom, width, size),
                        size * 0.8,
                        Color::new(r, g, b, a),
                        FontWeight::Semibold,
                        align,
                    );
                }
                None
            }
            (None, false, _) => Some(match choice.style {
                HudStyle::Classic => "SJK's classic layout: drawn by SJK, no game files needed.",
                _ => "SJK's radial layout: drawn by SJK, no game files needed.",
            }),
            (None, true, true) => Some("No preview: these files describe no HUD SJK can draw."),
            (None, true, false) => Some("Drawing the preview..."),
        };
        if let Some(note) = note {
            self.line(
                note,
                Rect::new(
                    frame.x,
                    frame.y + frame.height * 0.5 - 8.0 * s,
                    frame.width,
                    16.0 * s,
                ),
                12.0 * s,
                palette.text,
                FontWeight::Regular,
                TextAlign::Center,
            );
        }
        self.outline(frame, palette.border, 1.0 * s);
        let [x, y, w, h] = PREVIEW;
        self.line(
            &choice.label,
            place.rect([x, y + h + 8.0, w, 16.0]),
            15.0 * s,
            palette.focus,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let in_use = self.hud.current() == Some(self.hud.highlight);
        let origin = if in_use {
            format!("{}   \u{b7}   in use", choice.origin)
        } else {
            choice.origin.clone()
        };
        self.line(
            &origin,
            place.rect([x, y + h + 28.0, w, 13.0]),
            11.0 * s,
            palette.text,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    fn fill(&mut self, rect: Rect, color: Color) {
        let _ = self
            .ui
            .draw_list_mut()
            .push(DrawCommand::SolidRect { rect, color });
    }

    fn outline(&mut self, rect: Rect, color: Color, width: f32) {
        let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
            rect,
            radius: 0.0,
            width,
            color,
        });
    }

    fn line(
        &mut self,
        text: &str,
        rect: Rect,
        size: f32,
        color: Color,
        weight: FontWeight,
        align: TextAlign,
    ) {
        let rect = Rect::new(
            rect.x,
            rect.y + (rect.height - size * 1.25) * 0.5,
            rect.width,
            size * 1.25,
        );
        self.ui
            .text_aligned(text, rect, size, color, weight, 0.3 * size / 12.0, align);
    }
}

/// The picker's colours: retail's option panel (`setup.menu`) on the
/// classic menus, the theme's otherwise.
struct Palette {
    art: Option<ArtSet>,
    scrim: Color,
    panel: Color,
    border: Color,
    title: Color,
    text: Color,
    focus: Color,
    highlight: Color,
    in_use: Color,
    hint: Color,
}

impl Palette {
    fn new(art: Option<ArtSet>, theme: sjk_ui::Theme) -> Self {
        use crate::menu::classic::panel::{OPTION, focus_text};
        use crate::menu::classic::view::{GOLD, HINT, ink};
        match art {
            Some(_) => Self {
                art,
                scrim: ink(0.7),
                panel: Color::new(0.0, 0.0, 0.25, 0.85),
                border: Color::new(0.0, 0.0, 0.6, 1.0),
                title: Color::new(0.549, 0.854, 1.0, 1.0),
                text: OPTION,
                focus: focus_text(),
                highlight: Color::new(1.0, 1.0, 1.0, 0.12),
                in_use: GOLD,
                hint: HINT,
            },
            None => Self {
                art,
                scrim: Color::new(0.0, 0.0, 0.0, 0.6),
                panel: theme.surface_strong,
                border: Color::new(theme.accent.r, theme.accent.g, theme.accent.b, 0.45),
                title: theme.foreground,
                text: theme.muted,
                focus: theme.foreground,
                highlight: Color::new(theme.accent.r, theme.accent.g, theme.accent.b, 0.2),
                in_use: theme.accent,
                hint: theme.muted,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu_hud::STYLE_CVAR;
    use crate::menu_hud::choices::tests::installed;

    fn console() -> (tempfile::TempDir, ViewerConsole) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        (directory, console)
    }

    fn hud_tab(menu: &mut SettingsMenu, console: &ViewerConsole) -> usize {
        menu.open_tab(console, SettingsMenu::tab_index("HUD").unwrap());
        menu.rows()
            .iter()
            .position(|setting| matches!(setting.kind, ValueKind::HudPicker))
            .unwrap()
    }

    #[test]
    fn the_hud_row_names_the_hud_and_steps_through_the_list() {
        let (_directory, mut console) = console();
        let vfs = Arc::new(installed());
        let shaders = sjk_shader::ShaderCatalog::default();
        let mut menu = SettingsMenu::new();
        let row = hud_tab(&mut menu, &console);
        // A fresh profile uses the game HUD: the pack installed last.
        menu.service_hud_picker(Some(&vfs), &shaders, |_| {});
        assert_eq!(menu.values[row], "TheRisqe Radial HUD");
        menu.selected = row;
        menu.adjust(&mut console, 1);
        assert_eq!(menu.values[row], "Elegance HUD");
        assert_eq!(console.text_value(FILES_CVAR), Some("ui/elegance_hud.txt"));
        assert_eq!(console.text_value(PACK_CVAR), Some(""));
        // Text only, then SJK's radial HUD, which needs no pack.
        for _ in 0..2 {
            menu.adjust(&mut console, 1);
        }
        assert_eq!(menu.values[row], "SJK radial");
        assert_eq!(console.text_value(STYLE_CVAR), Some("radial"));
        // SJK's classic layout, then round to the first.
        for _ in 0..2 {
            menu.adjust(&mut console, 1);
        }
        assert_eq!(menu.values[row], "Jedi Academy");
        assert_eq!(console.text_value(STYLE_CVAR), Some("game"));
        assert_eq!(console.text_value(PACK_CVAR), Some("assets1.pk3"));
        assert_eq!(console.text_value(FILES_CVAR), Some("ui/jahud.txt"));
        menu.adjust(&mut console, -1);
        assert_eq!(menu.values[row], "SJK classic");
        assert_eq!(console.text_value(STYLE_CVAR), Some("classic"));
    }

    #[test]
    fn the_picker_opens_on_the_hud_in_use_and_previews_what_is_highlighted() {
        let (_directory, mut console) = console();
        console.set_cvar(PACK_CVAR, "JoF_AssetsExtra.pk3");
        let vfs = Arc::new(installed());
        let shaders = sjk_shader::ShaderCatalog::default();
        let mut menu = SettingsMenu::new();
        let row = hud_tab(&mut menu, &console);
        menu.service_hud_picker(Some(&vfs), &shaders, |_| {});
        menu.selected = row;
        menu.open_hud_picker(&console);
        assert!(menu.hud.is_open());
        assert_eq!(menu.hud.highlight, 1, "JoF AssetsExtra is in use");
        // The highlighted HUD's preview renders and is uploaded once.
        let mut uploads = 0;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while menu.hud.shown != Some(1) && std::time::Instant::now() < deadline {
            menu.service_hud_picker(Some(&vfs), &shaders, |image| {
                assert_eq!(image.size, preview::SIZE);
                uploads += 1;
            });
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        menu.service_hud_picker(Some(&vfs), &shaders, |_| uploads += 1);
        assert_eq!(uploads, 1);
        // SJK's layouts have no preview to render.
        menu.hud_picker_key(KeyCode::End, false, &mut console);
        menu.service_hud_picker(Some(&vfs), &shaders, |_| panic!("no preview"));
        assert!(menu.hud.rendering.is_none());
        // Escape leaves the HUD as it was; Enter uses the highlighted one.
        menu.hud_picker_key(KeyCode::Escape, false, &mut console);
        assert!(!menu.hud.is_open());
        assert_eq!(console.text_value(STYLE_CVAR), Some("game"));
        menu.open_hud_picker(&console);
        menu.service_hud_picker(Some(&vfs), &shaders, |_| {});
        menu.hud_picker_key(KeyCode::ArrowUp, false, &mut console);
        menu.hud_picker_key(KeyCode::Enter, false, &mut console);
        assert!(!menu.hud.is_open());
        assert_eq!(console.text_value(PACK_CVAR), Some("assets1.pk3"));
        assert_eq!(menu.values[row], "Jedi Academy");
    }
}
