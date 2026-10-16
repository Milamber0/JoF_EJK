//! Installed PK3 browser with restart-applied enable/disable choices.
use crate::assets::search_paths::{
    self,
    pack_policy::{self, Disabled},
};
use crate::menu::sjk::{Frame, TextTarget, color, kit, text};
use crate::menu_widgets::{BACK_TOKEN, MenuCanvas, TextFamily};
use sjk_ui::{FontWeight, InputEvent, TextAlign, UiEventKind};
use std::path::Path;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

pub(crate) const COMMAND: &str = "assetbrowser";
pub(crate) const HELP: &str =
    "Browse installed PK3 packs and enable or disable them for the next client start";
const ROWS: usize = 10;

struct Pack {
    key: String,
    label: String,
    required: bool,
}

pub(crate) enum Action {
    None,
    Close,
    Changed,
}

pub(crate) struct Browser {
    pub(crate) open: bool,
    pub(crate) owns_console: bool,
    packs: Vec<Pack>,
    disabled: Disabled,
    selected: usize,
    first: usize,
    status: String,
    ui: MenuCanvas,
}

impl Default for Browser {
    fn default() -> Self {
        Self {
            open: false,
            owns_console: false,
            packs: Vec::new(),
            disabled: Disabled::default(),
            selected: 0,
            first: 0,
            status: String::new(),
            ui: MenuCanvas::with_text_capacity(64),
        }
    }
}

impl Browser {
    pub(crate) fn open(
        &mut self,
        game_data: &Path,
        console: &crate::console::ViewerConsole,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let options = search_paths::Options::from_console(console)?;
        let mut directories = options.directories(game_data)?;
        let supplemental = game_data.join("EternalJK");
        if !directories.contains(&supplemental) {
            directories.push(supplemental);
        }
        self.packs.clear();
        for directory in directories.into_iter().filter(|path| path.is_dir()) {
            for archive in sjk_vfs::pk3_search_order(&directory)? {
                let Some(key) = pack_policy::key(&archive) else {
                    continue;
                };
                if self.packs.iter().any(|pack| pack.key == key) {
                    continue;
                }
                let label = format!(
                    "{}/{}",
                    directory.file_name().unwrap().to_string_lossy(),
                    archive.file_name().unwrap().to_string_lossy()
                );
                self.packs.push(Pack {
                    required: pack_policy::retail(&key),
                    key,
                    label,
                });
            }
        }
        self.packs.sort_by(|a, b| a.key.cmp(&b.key));
        self.disabled = Disabled::parse(console.text_cvar(pack_policy::CVAR).unwrap_or("[]"))?;
        self.selected = 0;
        self.first = 0;
        self.status = "Changes apply when SJK restarts. Files stay in place.".into();
        self.open = true;
        Ok(())
    }

    pub(crate) fn saved(&self) -> String {
        self.disabled.saved()
    }
    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    fn select(&mut self, index: usize) {
        self.selected = index.min(self.packs.len().saturating_sub(1));
        if self.selected < self.first {
            self.first = self.selected;
        }
        if self.selected >= self.first + ROWS {
            self.first = self.selected + 1 - ROWS;
        }
    }

    fn toggle(&mut self) -> Action {
        let Some(pack) = self.packs.get(self.selected) else {
            return Action::None;
        };
        if pack.required {
            self.status = "This retail pack is required and stays enabled.".into();
            return Action::None;
        }
        self.disabled.toggle(&pack.key);
        self.status = "Saved. Restart SJK to apply your pack choices.".into();
        Action::Changed
    }

    pub(crate) fn key(&mut self, event: &KeyEvent) -> Action {
        if event.state != ElementState::Pressed || event.repeat {
            return Action::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return Action::None;
        };
        match key {
            KeyCode::Escape => Action::Close,
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => self.toggle(),
            KeyCode::ArrowUp => {
                self.select(self.selected.saturating_sub(1));
                Action::None
            }
            KeyCode::ArrowDown => {
                self.select(self.selected + 1);
                Action::None
            }
            KeyCode::PageUp => {
                self.select(self.selected.saturating_sub(ROWS));
                Action::None
            }
            KeyCode::PageDown => {
                self.select(self.selected + ROWS);
                Action::None
            }
            _ => Action::None,
        }
    }

    pub(crate) fn pointer(&mut self, event: InputEvent) -> Action {
        let Some(event) = self.ui.pointer(event) else {
            return Action::None;
        };
        if event.kind == UiEventKind::Wheel {
            let direction = event.delta.map_or(0, |delta| -delta.y.signum() as isize);
            self.select(self.selected.saturating_add_signed(direction * 3));
            return Action::None;
        }
        if event.kind != UiEventKind::Activate {
            return Action::None;
        }
        match event.token {
            Some(BACK_TOKEN) => Action::Close,
            Some(row) if usize::from(row) < ROWS => {
                self.select(self.first + usize::from(row));
                self.toggle()
            }
            _ => Action::None,
        }
    }

    pub(crate) fn append(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        let frame = Frame::new(viewport);
        self.ui.begin_transparent(viewport);
        kit::scrim(&mut self.ui, viewport);
        kit::card(&mut self.ui, &frame, [420.0, 190.0, 1080.0, 700.0]);
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("Asset browser"),
            frame.rect(452.0, 220.0, 800.0, 50.0),
            36.0 * frame.s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Installed PK3 packs · choices for next start"),
            frame.rect(452.0, 282.0, 960.0, 28.0),
            18.0 * frame.s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        self.ui
            .scroll_region(910, frame.rect(444.0, 330.0, 1024.0, 430.0));
        for (slot, pack) in self.packs.iter().skip(self.first).take(ROWS).enumerate() {
            let top = 330.0 + slot as f32 * 43.0;
            let selected = self.first + slot == self.selected;
            if selected {
                kit::band(&mut self.ui, &frame, [444.0, top, 1024.0, 41.0]);
            }
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{}", pack.label),
                frame.rect(460.0, top + 7.0, 730.0, 26.0),
                18.0 * frame.s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
            let state = if pack.required {
                "Required"
            } else if self.disabled.contains(&pack.key) {
                "Disabled"
            } else {
                "Enabled"
            };
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{state}"),
                frame.rect(1220.0, top + 7.0, 216.0, 26.0),
                18.0 * frame.s,
                if self.disabled.contains(&pack.key) {
                    color::MUTED
                } else {
                    color::GOLD_BRIGHT
                },
                FontWeight::Regular,
                TextAlign::End,
            );
            self.ui
                .hit_region(slot as u16, frame.rect(444.0, top, 1024.0, 41.0));
        }
        if self.packs.is_empty() {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("No installed PK3 packs found."),
                frame.rect(452.0, 330.0, 960.0, 30.0),
                18.0 * frame.s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{}", self.status),
            frame.rect(452.0, 784.0, 960.0, 28.0),
            18.0 * frame.s,
            color::GOLD_BRIGHT,
            FontWeight::Regular,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Up/Down / wheel to browse · Enter / click to toggle · Esc to close"),
            frame.rect(452.0, 834.0, 950.0, 28.0),
            16.0 * frame.s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        self.ui
            .hit_region(BACK_TOKEN, frame.rect(1400.0, 218.0, 64.0, 42.0));
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Close"),
            frame.rect(1400.0, 225.0, 64.0, 28.0),
            16.0 * frame.s,
            color::TEXT,
            FontWeight::Regular,
            TextAlign::End,
        );
        self.ui
            .finish(self.selected.saturating_sub(self.first) as u16);
        target.append(&self.ui, viewport);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_browser_toggle_survives_profile_reload_and_required_packs_stay_enabled() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("base");
        std::fs::create_dir_all(&base).unwrap();
        // Browser inventory does not read/decompress archives.
        std::fs::write(base.join("My DL-44.pk3"), []).unwrap();
        std::fs::write(base.join("assets0.pk3"), []).unwrap();
        let config = directory.path().join("config.cfg");
        let mut console = crate::console::ViewerConsole::new(config.clone()).unwrap();
        let mut browser = Browser::default();
        browser.open(directory.path(), &console).unwrap();
        browser.select(
            browser
                .packs
                .iter()
                .position(|pack| pack.key == "base/my dl-44.pk3")
                .unwrap(),
        );
        assert!(matches!(browser.toggle(), Action::Changed));
        assert!(console.set_cvar(pack_policy::CVAR, &browser.saved()));
        drop(console);
        let console = crate::console::ViewerConsole::new(config).unwrap();
        browser.open(directory.path(), &console).unwrap();
        assert!(browser.disabled.contains("base/my dl-44.pk3"));
        browser.select(browser.packs.iter().position(|pack| pack.required).unwrap());
        assert!(matches!(browser.toggle(), Action::None));
        assert!(!browser.disabled.contains("base/assets0.pk3"));
    }

    #[test]
    #[ignore = "needs a GPU and JKA_GAME_DATA; retail assets are external"]
    fn the_installed_pack_browser_renders_on_a_gpu() {
        crate::world_shot::on_big_stack(|| {
            let (mut gpu, _profile) = crate::world_shot::open(
                "maps/mp/duel6.bsp",
                [1280, 960],
                None,
                &[("r_hdr", "0"), ("cg_materialMaps", "0")],
            )
            .expect("a GPU adapter");
            gpu.console
                .as_mut()
                .unwrap()
                .open_asset_browser(&gpu.game_data)
                .unwrap();
            assert!(matches!(
                gpu.render(&mut None),
                crate::gpu_context::FrameStatus::Rendered
            ));
            let console = gpu.console.as_ref().unwrap();
            assert!(console.covers_frame());
            assert!(console.sjk_page_open());
            assert!(!console.draw_list().commands().is_empty());
            if let Some(path) = std::env::var_os("SJK_ASSET_BROWSER_SHOT") {
                crate::world_shot::frame(&mut gpu, 2).save(path).unwrap();
            }
        });
    }
}
