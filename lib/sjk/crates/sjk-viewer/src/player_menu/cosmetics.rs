//! The cosmetics window's state (JoF EJK's `ingame_cosmetics`): the
//! installed hats and capes, what is worn, and each list's cursor.
//!
//! Wearing is a cvar edit, as in JoF EJK: the name goes after the saber
//! colour of `color1` (hat) or `color2` (cape), which every other player
//! receives with the clientinfo. `color1` and `color2` are the truth: the
//! `cosmetics` command writes them too, so they are read again each time
//! the window opens.

use crate::console::ViewerConsole;
use crate::cosmetics::{Catalog, VISIBILITY_CVAR, Visibility};
use sjk_client::CosmeticSlot;
use sjk_vfs::VirtualFileSystem;
use std::sync::Arc;

/// Show Cosmetics' order: On, Only Me, Off (the `cvarFloatList` of
/// `ingame_cosmetics.menu`).
const VISIBILITY_ORDER: [Visibility; 3] = [Visibility::On, Visibility::OnlyMe, Visibility::Off];

pub(super) struct CosmeticsMenu {
    /// Listed once per file system, the first time the window opens.
    catalog: Option<(Arc<VirtualFileSystem>, Catalog)>,
    /// Worn names, as `color1`/`color2` hold them.
    worn: [String; 2],
    /// Each list's highlighted row (keyboard and pointer).
    pub(super) cursor: [usize; 2],
    /// Each list's first visible row.
    pub(super) scroll: [usize; 2],
    /// `cg_cosmetics`, read on open and written by [`Self::cycle_visibility`].
    visibility: Visibility,
}

impl CosmeticsMenu {
    pub(super) fn new() -> Self {
        Self {
            catalog: None,
            worn: [String::new(), String::new()],
            cursor: [0; 2],
            scroll: [0; 2],
            visibility: Visibility::On,
        }
    }

    /// Read what is worn; list the installed pieces of `vfs` if not yet.
    pub(super) fn open(&mut self, console: &ViewerConsole, vfs: Option<&Arc<VirtualFileSystem>>) {
        if let Some(vfs) = vfs
            && !self
                .catalog
                .as_ref()
                .is_some_and(|(listed, _)| Arc::ptr_eq(listed, vfs))
        {
            self.catalog = Some((Arc::clone(vfs), Catalog::scan(vfs)));
        }
        self.read_worn(console);
        self.visibility = Visibility::from_cvar(console.integer_cvar(VISIBILITY_CVAR).unwrap_or(1));
        for slot in CosmeticSlot::ALL {
            let index = slot.index();
            self.cursor[index] = self.worn_position(slot).unwrap_or(0);
            self.scroll[index] = 0;
        }
    }

    /// Read `color1`/`color2` again.
    pub(super) fn read_worn(&mut self, console: &ViewerConsole) {
        for slot in CosmeticSlot::ALL {
            let worn = &mut self.worn[slot.index()];
            worn.clear();
            if let Some(name) = console
                .text_value(slot.cvar())
                .and_then(|value| sjk_client::split_color_value(value).1)
            {
                worn.push_str(name);
            }
        }
    }

    pub(super) fn catalog(&self) -> Option<&Catalog> {
        self.catalog.as_ref().map(|(_, catalog)| catalog)
    }

    /// The installed pieces of `slot`, in list order.
    pub(super) fn count(&self, slot: CosmeticSlot) -> usize {
        self.catalog()
            .map_or(0, |catalog| catalog.pieces(slot).len())
    }

    /// The name worn in `slot`, if any.
    pub(super) fn worn(&self, slot: CosmeticSlot) -> Option<&str> {
        let worn = self.worn[slot.index()].as_str();
        (!worn.is_empty()).then_some(worn)
    }

    /// How the menu names what is worn in `slot`: the listed name, else the
    /// worn name itself (a piece this client does not have).
    pub(super) fn worn_label(&self, slot: CosmeticSlot) -> Option<String> {
        let worn = self.worn(slot)?;
        Some(
            self.worn_position(slot)
                .and_then(|index| self.catalog()?.pieces(slot).get(index))
                .map_or_else(
                    || crate::cosmetics::display_name(worn),
                    |piece| piece.display.clone(),
                ),
        )
    }

    /// List row of the worn piece.
    pub(super) fn worn_position(&self, slot: CosmeticSlot) -> Option<usize> {
        self.catalog()?.position(slot, self.worn(slot)?)
    }

    /// Click on row `index`: wear it, or take it off when it is worn.
    pub(super) fn toggle(&mut self, console: &mut ViewerConsole, slot: CosmeticSlot, index: usize) {
        let Some(name) = self
            .catalog()
            .and_then(|catalog| catalog.pieces(slot).get(index))
            .map(|piece| piece.name.clone())
        else {
            return;
        };
        self.cursor[slot.index()] = index;
        let wearing = self
            .worn(slot)
            .is_some_and(|worn| worn.eq_ignore_ascii_case(&name));
        self.set(console, slot, (!wearing).then_some(name.as_str()));
    }

    /// Wear piece `index` of `slot`, or nothing in it (`None`): the SJK UI's
    /// picker, where a click on a piece always wears it.
    pub(super) fn wear(
        &mut self,
        console: &mut ViewerConsole,
        slot: CosmeticSlot,
        index: Option<usize>,
    ) {
        let name = match index {
            Some(index) => {
                let Some(name) = self
                    .catalog()
                    .and_then(|catalog| catalog.pieces(slot).get(index))
                    .map(|piece| piece.name.clone())
                else {
                    return;
                };
                self.cursor[slot.index()] = index;
                Some(name)
            }
            None => None,
        };
        self.set(console, slot, name.as_deref());
    }

    /// Whether JoF's catalogue has pieces of `slot` this client has not installed.
    pub(super) fn has_missing(&self, slot: CosmeticSlot) -> bool {
        self.catalog()
            .is_some_and(|catalog| catalog.has_missing(slot))
    }

    /// The listed name of piece `index` of `slot`.
    pub(super) fn display(&self, slot: CosmeticSlot, index: usize) -> Option<&str> {
        self.catalog()?
            .pieces(slot)
            .get(index)
            .map(|piece| piece.display.as_str())
    }

    /// Take off both pieces (JoF's Remove All).
    pub(super) fn clear(&mut self, console: &mut ViewerConsole) {
        for slot in CosmeticSlot::ALL {
            self.set(console, slot, None);
        }
    }

    /// Move `slot`'s cursor by `step` rows, within the list.
    pub(super) fn move_cursor(&mut self, slot: CosmeticSlot, step: isize) {
        let count = self.count(slot);
        if count == 0 {
            return;
        }
        let cursor = &mut self.cursor[slot.index()];
        *cursor = cursor.saturating_add_signed(step).min(count - 1);
    }

    /// `cg_cosmetics` as the window shows it.
    pub(super) fn visibility(&self) -> Visibility {
        self.visibility
    }

    /// Step `cg_cosmetics` through On, Only Me and Off.
    pub(super) fn cycle_visibility(&mut self, console: &mut ViewerConsole, direction: isize) {
        let order = VISIBILITY_ORDER;
        let index = order
            .iter()
            .position(|visibility| *visibility == self.visibility)
            .unwrap_or(0);
        self.visibility = order[super::controller::wrap(index, direction, order.len())];
        console.set_cvar(VISIBILITY_CVAR, self.visibility.cvar_value());
    }

    /// Write `slot`'s cvar wearing `name`, keeping the saber colour.
    fn set(&mut self, console: &mut ViewerConsole, slot: CosmeticSlot, name: Option<&str>) {
        let colour = console
            .text_value(slot.cvar())
            .map_or(4, |value| sjk_client::split_color_value(value).0);
        let value = sjk_client::join_color_value(colour, name);
        console.set_cvar(slot.cvar(), &value);
        let worn = &mut self.worn[slot.index()];
        worn.clear();
        worn.push_str(name.unwrap_or_default());
    }
}
