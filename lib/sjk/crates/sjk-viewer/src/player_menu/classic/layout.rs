//! Entries and geometry of the classic profile pages, from the retail
//! `ui/jamp` menus: `player.menu`, `player2.menu` and `saber.menu` (full
//! screen), `ingame_player.menu`, `ingame_player2.menu`,
//! `ingame_saber.menu` and `ingame_playerforce.menu` (windows over a
//! match), and JoF EJK's `ingame_cosmetics.menu`. Rectangles are the retail
//! item rectangles on the 640x480 canvas; in-game ones are given relative to
//! their window, as retail wrote them, and offset by [`window`].
//!
//! SJK departs from retail where it adds to it: the Force page is a window
//! on both frames (retail had it in game only), as wide as the in-game
//! profile, with retail's templates down its left and the powers in two
//! columns with a detail panel; the profile page gains the Force and
//! Cosmetics buttons, and the cosmetics window lists hats and capes side by
//! side where JoF EJK drew its preview model.

use super::{ClassicPage, Frame};

/// One selectable entry of a classic profile page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Item {
    NavPlay,
    NavProfile,
    NavControls,
    NavSetup,
    Name,
    Team,
    /// SJK: the head grid's search field.
    Search,
    /// The head grid (`FEEDER_Q3HEADS`).
    Models,
    /// The Custom button leading to character creation.
    Custom,
    /// The in-game profile's Saber button.
    SaberButton,
    Species,
    /// Skin tint swatches.
    Tints,
    PartHead,
    PartTorso,
    PartLegs,
    /// The list of the selected part's variants.
    Parts,
    Single,
    Dual,
    Staff,
    Hilts,
    Hilts2,
    Blades,
    Blades2,
    /// SJK (JoF EJK): a custom blade colour slider, the first saber's red,
    /// green and blue (0..3), then the second's (3..6).
    Channel(u8),
    Exit,
    Back,
    /// APPLY: on to the next page, or back to the match in game.
    Apply,
    /// The saber page's second Apply: back to the main menu.
    ApplyMain,
    /// The profile's Force button (`configforce`): on to the Force page.
    ForceButton,
    /// The profile's Cosmetics button (JoF EJK): on to the cosmetics window.
    CosmeticsButton,
    /// The Force page's Light and Dark side cards.
    SideLight,
    SideDark,
    /// One power row of the Force page, by `forcePowers_t` index.
    Power(u8),
    /// Clear every level of the Force draft.
    ForceReset,
    /// Return the Force draft to the applied profile.
    ForceDiscard,
    /// Write the Force draft and return to the profile page.
    ForceApply,
    /// The Force page's template list (`FEEDER_FORCECFG`).
    Templates,
    /// The name a template is saved as (`ui_SaveFCF`).
    TemplateName,
    /// Save the draft as a template.
    TemplateSave,
    /// The cosmetics window's hat and cape lists.
    Hats,
    Capes,
    /// `cg_cosmetics`: whose cosmetics are drawn.
    CosmeticsShow,
    /// Take the hat and the cape off.
    CosmeticsClear,
}

impl Item {
    pub(crate) fn is_nav(self) -> bool {
        matches!(
            self,
            Self::NavPlay | Self::NavProfile | Self::NavControls | Self::NavSetup
        )
    }

    /// The part list a part button selects.
    pub(crate) fn part_axis(self) -> Option<usize> {
        match self {
            Self::PartHead => Some(0),
            Self::PartTorso => Some(1),
            Self::PartLegs => Some(2),
            _ => None,
        }
    }

    /// The Force power a power row edits.
    pub(crate) fn power(self) -> Option<usize> {
        match self {
            Self::Power(index) => Some(usize::from(index)),
            _ => None,
        }
    }

    /// Retail label (`strings/english/menus.str`); empty for lists and
    /// image buttons, which draw their own content.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::NavPlay => "PLAY",
            Self::NavProfile => "PROFILE",
            Self::NavControls => "SETTINGS",
            Self::NavSetup => "SJK",
            Self::Name => "Name:",
            Self::Team => "Team Color:",
            Self::Search => "Search:",
            Self::PartHead => "HEAD",
            Self::PartTorso => "TORSO",
            Self::PartLegs => "LEGS",
            Self::Single => "Standard Saber",
            Self::Dual => "Dual Sabers",
            Self::Staff => "Two-Handed Saber",
            Self::Exit => "EXIT",
            Self::Back => "Back",
            Self::Apply => "APPLY",
            Self::ApplyMain => "Apply",
            Self::CosmeticsButton => "Cosmetics",
            Self::ForceReset => "Reset",
            Self::ForceDiscard => "Discard",
            Self::ForceApply => "Apply Powers",
            Self::TemplateName => "Name:",
            Self::TemplateSave => "Save Template",
            Self::CosmeticsClear => "Remove All",
            Self::ForceButton
            | Self::SideLight
            | Self::SideDark
            | Self::Power(_)
            | Self::Templates
            | Self::Hats
            | Self::Capes
            | Self::CosmeticsShow
            | Self::Models
            | Self::Custom
            | Self::SaberButton
            | Self::Species
            | Self::Tints
            | Self::Parts
            | Self::Hilts
            | Self::Hilts2
            | Self::Blades
            | Self::Blades2
            | Self::Channel(_) => "",
        }
    }

    /// Retail description line (`descText`).
    pub(crate) fn hint(self) -> &'static str {
        match self {
            Self::NavPlay => "Start playing now!",
            Self::NavProfile => "Configure character settings.",
            Self::NavControls => "Key bindings and every option, with search",
            Self::NavSetup => "Changelog, credits and updates",
            Self::Name => "Enter your name here.",
            Self::Team => "Choose the color for your model's skin.",
            Self::Search => "Type part of a model's name to list only the models matching it.",
            Self::Models => "Choose the model for your character.",
            Self::Custom => "Create your own character.",
            Self::SaberButton => "Configure your lightsaber.",
            Self::Species => "Choose a species.",
            Self::Tints => "Choose the color for your model's skin.",
            Self::PartHead => "Select a head style.",
            Self::PartTorso => "Select a torso style.",
            Self::PartLegs => "Select a legs style.",
            Self::Parts => "Choose your character's look.",
            Self::Single => "Use one one-handed saber, choice of 3 fighting styles.",
            Self::Dual => "Use 2 one-handed sabers, only one fighting style.",
            Self::Staff => "Use one two-handed saber, only one fighting style.",
            Self::Hilts => "Select a hilt.",
            Self::Hilts2 => "Select a second hilt.",
            Self::Blades => "Select a blade color.",
            Self::Blades2 => "Select a second blade color.",
            Self::Channel(index) => CHANNEL_HINTS[usize::from(index % 3)],
            Self::Exit => "Leave Jedi Academy.",
            Self::Back => "Back to profile menu.",
            Self::Apply => "Apply changes to player and go to saber selection.",
            Self::ApplyMain => "Apply saber changes and return to Main Menu.",
            Self::ForceButton => "Set up the Force Abilities for your character.",
            Self::CosmeticsButton => "Wear a hat or a cape.",
            Self::SideLight | Self::SideDark => {
                "Choose the path of the Light Side or the Dark Side."
            }
            Self::Power(index) => POWER_HINTS
                .get(usize::from(index))
                .copied()
                .unwrap_or_default(),
            Self::ForceReset => "Take back every point and start over.",
            Self::ForceDiscard => "Return to the powers you last applied.",
            Self::ForceApply => "Make these changes to your character's Force Abilities.",
            Self::Templates => "Choose a pre-made allocation of Force powers.",
            Self::TemplateName => "Enter the title for your template.",
            Self::TemplateSave => "Save the current Force setup as a template.",
            Self::Hats | Self::Capes => "Click to wear, click again to take off.",
            Self::CosmeticsShow => "Show cosmetics on everyone, only yourself, or nobody.",
            Self::CosmeticsClear => "Take off the hat and the cape.",
        }
    }
}

/// The custom colour sliders' descriptions, red, green and blue.
const CHANNEL_HINTS: [&str; 3] = [
    "Red of your own blade color; a swatch brings a stock color back.",
    "Green of your own blade color; a swatch brings a stock color back.",
    "Blue of your own blade color; a swatch brings a stock color back.",
];

/// Retail power descriptions (`descText` of `ingame_playerforce.menu`), in
/// `forcePowers_t` order.
const POWER_HINTS: [&str; 18] = [
    "Heal your body with the Force.",
    "Leap to amazing heights by holding the Jump button.",
    "Move at an accelerated rate.",
    "Push your foes away and repel projectiles.",
    "Pull your foes to you.",
    "Render yourself invisible to selected targets.",
    "Immobilize opponents in an agonizing grip of death.",
    "Unleash deadly electrical attacks against your foes.",
    "Become a nearly unstoppable juggernaut.",
    "Create an aura that protects from physical attacks.",
    "Protect against Force attacks and gain power from them.",
    "Channel the Force to heal your allies.",
    "Channel the Force to boost the power of your allies.",
    "Drain the force power of opponents to add to your health.",
    "See enemies at all times, and even dodge sniper shots.",
    "Use more powerful lightsaber attacks. (Required to possess saber)",
    "Use the lightsaber to deflect incoming saber attacks and projectiles.",
    "Throw your lightsaber to damage distant targets.",
];

/// The Force page's power columns, in `forcePowers_t` indices and retail
/// row order: the neutral powers, the saber skills, and each side's five.
pub(crate) const NEUTRAL_POWERS: [u8; 5] = [1, 3, 4, 2, 14];
pub(crate) const SABER_POWERS: [u8; 3] = [15, 16, 17];
pub(crate) const LIGHT_POWERS: [u8; 5] = [10, 0, 9, 5, 11];
pub(crate) const DARK_POWERS: [u8; 5] = [6, 13, 7, 8, 12];

const PLAYER_FULL: [Item; 13] = [
    Item::NavPlay,
    Item::NavProfile,
    Item::NavControls,
    Item::NavSetup,
    Item::Name,
    Item::Team,
    Item::Search,
    Item::Models,
    Item::Custom,
    Item::ForceButton,
    Item::CosmeticsButton,
    Item::Exit,
    Item::Apply,
];
const PLAYER_IN_GAME: [Item; 9] = [
    Item::Name,
    Item::Team,
    Item::Search,
    Item::Models,
    Item::Custom,
    Item::CosmeticsButton,
    Item::ForceButton,
    Item::SaberButton,
    Item::Apply,
];
const CHARACTER_FULL: [Item; 12] = [
    Item::NavPlay,
    Item::NavProfile,
    Item::NavControls,
    Item::NavSetup,
    Item::Species,
    Item::Tints,
    Item::PartHead,
    Item::PartTorso,
    Item::PartLegs,
    Item::Parts,
    Item::Back,
    Item::Apply,
];
const CHARACTER_IN_GAME: [Item; 8] = [
    Item::Species,
    Item::Tints,
    Item::PartHead,
    Item::PartTorso,
    Item::PartLegs,
    Item::Parts,
    Item::Back,
    Item::Apply,
];
const SABER_FULL: [Item; 14] = [
    Item::NavPlay,
    Item::NavProfile,
    Item::NavControls,
    Item::NavSetup,
    Item::Single,
    Item::Dual,
    Item::Staff,
    Item::Hilts,
    Item::Blades,
    Item::Channel(0),
    Item::Channel(1),
    Item::Channel(2),
    Item::Exit,
    // Retail's `saber.menu` has only EXIT and one Apply: its middle `apply`
    // button (255 444) sits in a commented-out block.
    Item::ApplyMain,
];
const SABER_FULL_DUAL: [Item; 19] = [
    Item::NavPlay,
    Item::NavProfile,
    Item::NavControls,
    Item::NavSetup,
    Item::Single,
    Item::Dual,
    Item::Staff,
    Item::Hilts,
    Item::Hilts2,
    Item::Blades,
    Item::Blades2,
    Item::Channel(0),
    Item::Channel(1),
    Item::Channel(2),
    Item::Channel(3),
    Item::Channel(4),
    Item::Channel(5),
    Item::Exit,
    Item::ApplyMain,
];
const SABER_IN_GAME: [Item; 9] = [
    Item::Single,
    Item::Dual,
    Item::Staff,
    Item::Hilts,
    Item::Blades,
    Item::Channel(0),
    Item::Channel(1),
    Item::Channel(2),
    Item::Apply,
];
const SABER_IN_GAME_DUAL: [Item; 14] = [
    Item::Single,
    Item::Dual,
    Item::Staff,
    Item::Hilts,
    Item::Hilts2,
    Item::Blades,
    Item::Channel(0),
    Item::Channel(1),
    Item::Channel(2),
    Item::Blades2,
    Item::Channel(3),
    Item::Channel(4),
    Item::Channel(5),
    Item::Apply,
];

/// The Force page's entries for one side: the side cards, the left column
/// (neutral powers, then the saber skills), the side's column, and the
/// buttons; `nav` adds the main menu's navigation row and Back.
const fn force_items<const N: usize>(side: [u8; 5], nav: bool) -> [Item; N] {
    // A full-screen page ends on Back, which the array starts filled with.
    let mut items = [Item::Back; N];
    let mut at = 0;
    if nav {
        items[0] = Item::NavPlay;
        items[1] = Item::NavProfile;
        items[2] = Item::NavControls;
        items[3] = Item::NavSetup;
        at = 4;
    }
    items[at] = Item::SideLight;
    items[at + 1] = Item::SideDark;
    items[at + 2] = Item::Templates;
    items[at + 3] = Item::TemplateName;
    items[at + 4] = Item::TemplateSave;
    at += 5;
    let mut index = 0;
    while index < NEUTRAL_POWERS.len() {
        items[at] = Item::Power(NEUTRAL_POWERS[index]);
        at += 1;
        index += 1;
    }
    index = 0;
    while index < SABER_POWERS.len() {
        items[at] = Item::Power(SABER_POWERS[index]);
        at += 1;
        index += 1;
    }
    index = 0;
    while index < side.len() {
        items[at] = Item::Power(side[index]);
        at += 1;
        index += 1;
    }
    items[at] = Item::ForceReset;
    items[at + 1] = Item::ForceDiscard;
    items[at + 2] = Item::ForceApply;
    items
}

const FORCE_LIGHT_IN_GAME: [Item; 21] = force_items(LIGHT_POWERS, false);
const FORCE_DARK_IN_GAME: [Item; 21] = force_items(DARK_POWERS, false);
const FORCE_LIGHT_FULL: [Item; 26] = force_items(LIGHT_POWERS, true);
const FORCE_DARK_FULL: [Item; 26] = force_items(DARK_POWERS, true);
const COSMETICS: [Item; 5] = [
    Item::Hats,
    Item::Capes,
    Item::CosmeticsShow,
    Item::CosmeticsClear,
    Item::Apply,
];

/// The page's entries in focus order (retail item order). `dark` picks the
/// Force page's side column.
pub(crate) fn items(page: ClassicPage, frame: Frame, dual: bool, dark: bool) -> &'static [Item] {
    match (page, frame, dual) {
        (ClassicPage::Force, Frame::Full, _) if dark => &FORCE_DARK_FULL,
        (ClassicPage::Force, Frame::Full, _) => &FORCE_LIGHT_FULL,
        (ClassicPage::Force, Frame::InGame, _) if dark => &FORCE_DARK_IN_GAME,
        (ClassicPage::Force, Frame::InGame, _) => &FORCE_LIGHT_IN_GAME,
        (ClassicPage::Cosmetics, _, _) => &COSMETICS,
        (ClassicPage::Player, Frame::Full, _) => &PLAYER_FULL,
        (ClassicPage::Player, Frame::InGame, _) => &PLAYER_IN_GAME,
        (ClassicPage::Character, Frame::Full, _) => &CHARACTER_FULL,
        (ClassicPage::Character, Frame::InGame, _) => &CHARACTER_IN_GAME,
        (ClassicPage::Saber, Frame::Full, false) => &SABER_FULL,
        (ClassicPage::Saber, Frame::Full, true) => &SABER_FULL_DUAL,
        (ClassicPage::Saber, Frame::InGame, false) => &SABER_IN_GAME,
        (ClassicPage::Saber, Frame::InGame, true) => &SABER_IN_GAME_DUAL,
    }
}

/// The retail window of an in-game page on the 640x480 canvas
/// (`ingame_player` 20 25 600 440, the others 105 40 430 425, JoF EJK's
/// `ingame_cosmetics` 105 40 430 400); the full screen for the main menu's
/// pages, except the Force page and the cosmetics window, which are windows
/// there too. The Force page is as wide as `ingame_player` (shortened on the
/// main menu to clear its bottom row).
pub(crate) fn window(page: ClassicPage, frame: Frame) -> [f32; 4] {
    match (frame, page) {
        (_, ClassicPage::Cosmetics) => [105.0, 40.0, 430.0, 400.0],
        (Frame::Full, ClassicPage::Force) => [20.0, 46.0, 600.0, 392.0],
        (Frame::InGame, ClassicPage::Force) => [20.0, 28.0, 600.0, 425.0],
        (Frame::Full, _) => [0.0, 0.0, 640.0, 480.0],
        (Frame::InGame, ClassicPage::Player) => [20.0, 25.0, 600.0, 440.0],
        (Frame::InGame, _) => [105.0, 40.0, 430.0, 425.0],
    }
}

/// Offset a window-relative retail rectangle onto the canvas.
pub(crate) fn place(page: ClassicPage, frame: Frame, [x, y, w, h]: [f32; 4]) -> [f32; 4] {
    let [wx, wy, _, _] = window(page, frame);
    [wx + x, wy + y, w, h]
}

/// Whether `item` sits on the canvas itself rather than in the page's
/// window: the navigation row, Exit, and the Force page's Back.
pub(crate) fn on_canvas(item: Item, page: ClassicPage, frame: Frame) -> bool {
    item.is_nav()
        || item == Item::Exit
        || (item == Item::Back && page == ClassicPage::Force && frame == Frame::Full)
}

/// Force page geometry, relative to its window.
pub(crate) mod force {
    /// Left edge of the left power column and of the side column, and the
    /// columns' width.
    pub(crate) const LEFT: f32 = 195.0;
    pub(crate) const RIGHT: f32 = 395.0;
    pub(crate) const COLUMN: f32 = 190.0;
    /// Side cards.
    pub(crate) const LIGHT_CARD: [f32; 4] = [LEFT, 56.0, COLUMN, 34.0];
    pub(crate) const DARK_CARD: [f32; 4] = [RIGHT, 56.0, COLUMN, 34.0];
    /// The points line and its meter.
    pub(crate) const POINTS: [f32; 4] = [LEFT, 94.0, RIGHT + COLUMN - LEFT, 14.0];
    pub(crate) const METER: [f32; 4] = [LEFT, 110.0, RIGHT + COLUMN - LEFT, 6.0];
    /// Column headings: templates, neutral, the side's, the saber skills'.
    pub(crate) const TEMPLATES_HEAD: [f32; 4] = [15.0, 56.0, 170.0, 14.0];
    pub(crate) const NEUTRAL_HEAD: [f32; 4] = [LEFT, 122.0, COLUMN, 14.0];
    pub(crate) const SIDE_HEAD: [f32; 4] = [RIGHT, 122.0, COLUMN, 14.0];
    pub(crate) const SABER_HEAD: [f32; 4] = [LEFT, 262.0, COLUMN, 14.0];
    /// The focused power's panel under the side column.
    pub(crate) const DETAIL: [f32; 4] = [RIGHT, 262.0, COLUMN, 86.0];
    /// The template list, the name field, Save, and the line saying how a
    /// save went.
    pub(crate) const TEMPLATES: [f32; 4] = [15.0, 74.0, 170.0, 224.0];
    pub(crate) const TEMPLATE_NAME: [f32; 4] = [15.0, 304.0, 170.0, 20.0];
    pub(crate) const TEMPLATE_SAVE: [f32; 4] = [15.0, 328.0, 170.0, 26.0];
    pub(crate) const TEMPLATE_NOTE: [f32; 4] = [15.0, 358.0, 170.0, 30.0];
    /// Row height of the template list.
    pub(crate) const TEMPLATE_ROW: f32 = 16.0;
    /// Height of a power row and the step between rows.
    pub(crate) const ROW: f32 = 22.0;
    pub(crate) const ROW_STEP: f32 = 24.0;
    /// Side of a level star (`UI_DrawForceStars`: 16, 4 apart) and the step.
    pub(crate) const STAR: f32 = 16.0;
    pub(crate) const STAR_STEP: f32 = 20.0;

    /// Row `row` of the column starting at `top`, `x` from the left.
    pub(crate) fn row(x: f32, top: f32, row: usize) -> [f32; 4] {
        [x, top + row as f32 * ROW_STEP, COLUMN, ROW]
    }

    /// Where level `level` (1 to 3) of the row `row` draws its star.
    pub(crate) fn star(row: [f32; 4], level: u8) -> [f32; 4] {
        let [x, y, w, h] = row;
        let right = x + w - 4.0;
        let left = right - 3.0 * STAR_STEP + (STAR_STEP - STAR);
        [
            left + f32::from(level.saturating_sub(1)) * STAR_STEP,
            y + (h - STAR) * 0.5,
            STAR,
            STAR,
        ]
    }
}

/// Window-relative row of Force power `index`.
pub(crate) fn power_row(index: u8) -> Option<[f32; 4]> {
    let column = |list: &[u8], x: f32, top: f32| {
        list.iter()
            .position(|power| *power == index)
            .map(|row| force::row(x, top, row))
    };
    column(&NEUTRAL_POWERS, force::LEFT, 138.0)
        .or_else(|| column(&SABER_POWERS, force::LEFT, 278.0))
        .or_else(|| column(&LIGHT_POWERS, force::RIGHT, 138.0))
        .or_else(|| column(&DARK_POWERS, force::RIGHT, 138.0))
}

/// Where a page shows the live model, on the canvas: character creation's
/// model item (`player2` 393 104 220 220, `ingame_player2` 300 84 110 110),
/// the cosmetics window's column right of its lists, and lightsaber
/// creation's band under its boxes, where retail spun the hilt, left of the
/// colour sliders (full page) or under them (in game).
pub(crate) fn preview_rect(page: ClassicPage, frame: Frame) -> Option<[f32; 4]> {
    let local = match (page, frame) {
        (ClassicPage::Character, Frame::Full) => return Some([393.0, 104.0, 220.0, 220.0]),
        (ClassicPage::Character, Frame::InGame) => [300.0, 84.0, 110.0, 110.0],
        (ClassicPage::Saber, Frame::Full) => return Some(SABER_PREVIEW),
        (ClassicPage::Saber, Frame::InGame) => [40.0, 286.0, 350.0, 70.0],
        (ClassicPage::Cosmetics, _) => COSMETICS_MODEL,
        _ => return None,
    };
    Some(place(page, frame, local))
}

/// Lightsaber creation's preview band, on the canvas: the lower box's left,
/// the colour sliders taking its right.
pub(crate) const SABER_PREVIEW: [f32; 4] = [24.0, 244.0, 420.0, 168.0];

/// The cosmetics window's preview column and the model inside it, relative
/// to the window.
pub(crate) const COSMETICS_PREVIEW: [f32; 4] = [285.0, 40.0, 130.0, 244.0];
pub(crate) const COSMETICS_MODEL: [f32; 4] = [287.0, 42.0, 126.0, 220.0];

/// Navigation row: `player.menu` puts it under the logo at y 126, the
/// creation pages along the top at y 16.
fn nav(page: ClassicPage, index: usize) -> [f32; 4] {
    let y = if page == ClassicPage::Player {
        126.0
    } else {
        16.0
    };
    [[7.0, 170.0, 340.0, 502.0][index], y, 130.0, 24.0]
}

/// The item's pointer target on the canvas (window offset applied).
pub(crate) fn rect(item: Item, page: ClassicPage, frame: Frame, dual: bool) -> [f32; 4] {
    use ClassicPage::{Character, Cosmetics, Force, Player, Saber};
    use Frame::{Full, InGame};
    let local = match (frame, page, item) {
        // The Force page (window-relative on both frames).
        (_, Force, Item::SideLight) => force::LIGHT_CARD,
        (_, Force, Item::SideDark) => force::DARK_CARD,
        (_, Force, Item::Power(index)) => power_row(index).unwrap_or_default(),
        (_, Force, Item::ForceReset) => [195.0, 360.0, 120.0, 28.0],
        (_, Force, Item::ForceDiscard) => [330.0, 360.0, 120.0, 28.0],
        (_, Force, Item::ForceApply) => [465.0, 360.0, 120.0, 28.0],
        (_, Force, Item::Templates) => force::TEMPLATES,
        (_, Force, Item::TemplateName) => force::TEMPLATE_NAME,
        (_, Force, Item::TemplateSave) => force::TEMPLATE_SAVE,
        (Full, Force, Item::Back) => [59.0, 444.0, 130.0, 24.0],
        // The cosmetics window (JoF EJK's `ingame_cosmetics`, lists side by side).
        (_, Cosmetics, Item::Hats) => [15.0, 60.0, 130.0, 224.0],
        (_, Cosmetics, Item::Capes) => [150.0, 60.0, 130.0, 224.0],
        (_, Cosmetics, Item::CosmeticsShow) => [115.0, 312.0, 200.0, 20.0],
        (_, Cosmetics, Item::CosmeticsClear) => [20.0, 345.0, 110.0, 32.0],
        (_, Cosmetics, Item::Apply) => [300.0, 345.0, 110.0, 32.0],
        (_, _, Item::NavPlay) => nav(page, 0),
        (_, _, Item::NavProfile) => nav(page, 1),
        (_, _, Item::NavControls) => nav(page, 2),
        (_, _, Item::NavSetup) => nav(page, 3),
        (_, _, Item::Exit) => [59.0, 444.0, 130.0, 24.0],
        // player.menu / ingame_player.menu
        (Full, Player, Item::Name) => [15.0, 163.0, 300.0, 28.0],
        (InGame, Player, Item::Name) => [20.0, 31.0, 300.0, 22.0],
        (Full, Player, Item::Team) => [50.0, 205.0, 160.0, 19.0],
        (InGame, Player, Item::Team) => [50.0, 77.0, 160.0, 13.0],
        // SJK: the search field right of Team Color, over the grid's right half.
        (Full, Player, Item::Search) => [250.0, 205.0, 184.0, 18.0],
        (InGame, Player, Item::Search) => [250.0, 78.0, 174.0, 12.0],
        (Full, Player, Item::Models) => [30.0, 224.0, 404.0, 194.0],
        (InGame, Player, Item::Models) => [20.0, 90.0, 404.0, 194.0],
        // SJK: Custom and Force side by side over the Cosmetics button.
        (Full, Player, Item::Custom) => [442.0, 234.0, 80.0, 80.0],
        (InGame, Player, Item::Custom) => [465.0, 160.0, 75.0, 75.0],
        (Full, Player, Item::ForceButton) => [537.0, 234.0, 80.0, 80.0],
        (InGame, Player, Item::ForceButton) => [275.0, 320.0, 75.0, 75.0],
        (Full, Player, Item::CosmeticsButton) => [449.0, 366.0, 160.0, 26.0],
        (InGame, Player, Item::CosmeticsButton) => [425.0, 250.0, 160.0, 26.0],
        (_, Player, Item::SaberButton) => [465.0, 322.0, 75.0, 75.0],
        (Full, Player, Item::Apply) => [455.0, 444.0, 130.0, 24.0],
        // Retail's 32-unit button (`5 412 105 32`) began above the 20-unit band
        // (`0 420 600 20`) it sits on, so its word hung high; centred on the band.
        (InGame, Player, Item::Apply) => [5.0, 420.0, 105.0, 20.0],
        // player2.menu / ingame_player2.menu
        (Full, Character, Item::Species) => [176.0, 92.0, 150.0, 16.0],
        (InGame, Character, Item::Species) => [161.0, 52.0, 150.0, 16.0],
        (Full, Character, Item::Tints) => [30.0, 168.0, 292.0, 48.0],
        (InGame, Character, Item::Tints) => [15.0, 104.0, 292.0, 48.0],
        (Full, Character, Item::PartHead) => [30.0, 280.0, 90.0, 16.0],
        (Full, Character, Item::PartTorso) => [126.0, 280.0, 90.0, 16.0],
        (Full, Character, Item::PartLegs) => [224.0, 280.0, 90.0, 16.0],
        (InGame, Character, Item::PartHead) => [15.0, 184.0, 90.0, 16.0],
        (InGame, Character, Item::PartTorso) => [111.0, 184.0, 90.0, 16.0],
        (InGame, Character, Item::PartLegs) => [209.0, 184.0, 90.0, 16.0],
        (Full, Character, Item::Parts) => [30.0, 306.0, 292.0, 93.0],
        (InGame, Character, Item::Parts) => [15.0, 206.0, 292.0, 93.0],
        (Full, Character, Item::Back) => [59.0, 444.0, 130.0, 24.0],
        (InGame, Character, Item::Back) => [30.0, 370.0, 110.0, 32.0],
        (Full, Character, Item::Apply) => [455.0, 444.0, 130.0, 24.0],
        (InGame, Character, Item::Apply) => [290.0, 370.0, 110.0, 32.0],
        // saber.menu / ingame_saber.menu
        (Full, Saber, Item::Single) => [32.0, 132.0, 180.0, 16.0],
        (Full, Saber, Item::Dual) => [32.0, 152.0, 180.0, 16.0],
        (Full, Saber, Item::Staff) => [32.0, 172.0, 180.0, 16.0],
        (InGame, Saber, Item::Single) => [15.0, 72.0, 180.0, 16.0],
        (InGame, Saber, Item::Dual) => [15.0, 88.0, 180.0, 16.0],
        (InGame, Saber, Item::Staff) => [15.0, 104.0, 180.0, 16.0],
        (Full, Saber, Item::Hilts) if dual => [240.0, 95.0, 160.0, 54.0],
        (Full, Saber, Item::Hilts) => [240.0, 95.0, 160.0, 120.0],
        (Full, Saber, Item::Hilts2) => [240.0, 165.0, 160.0, 54.0],
        (InGame, Saber, Item::Hilts) if dual => [200.0, 50.0, 160.0, 55.0],
        (InGame, Saber, Item::Hilts) => [200.0, 56.0, 160.0, 120.0],
        (InGame, Saber, Item::Hilts2) => [200.0, 120.0, 160.0, 55.0],
        (frame, Saber, Item::Channel(index)) => super::saber_rgb::rect(frame, index),
        (Full, Saber, Item::Blades) => [446.0, 124.0, 159.0, 24.0],
        (Full, Saber, Item::Blades2) => [446.0, 170.0, 159.0, 24.0],
        (InGame, Saber, Item::Blades) => [15.0, 197.0, 149.0, 24.0],
        (InGame, Saber, Item::Blades2) => [270.0, 197.0, 149.0, 24.0],
        (Full, Saber, Item::ApplyMain) => [455.0, 444.0, 130.0, 24.0],
        (InGame, Saber, Item::Apply) => [160.0, 360.0, 110.0, 32.0],
        _ => [0.0, 0.0, 0.0, 0.0],
    };
    if on_canvas(item, page, frame) {
        return local;
    }
    place(page, frame, local)
}

/// Tiles per row of the head grid: 64-unit cells in its 404-unit width.
pub(crate) const GRID_COLUMNS: usize = 6;
/// Cell side of the head grid, on the canvas.
pub(crate) const GRID_CELL: f32 = 64.0;
/// Cell side of the part lists (`elementwidth 72`).
pub(crate) const PART_CELL: f32 = 72.0;
/// Cell side of the tint list (`elementwidth 32`).
pub(crate) const TINT_CELL: f32 = 32.0;
/// Row height of the hilt lists (`elementheight 16`).
pub(crate) const HILT_ROW: f32 = 16.0;
/// Side of a blade colour swatch, and the step between swatches.
pub(crate) const SWATCH: f32 = 24.0;
pub(crate) fn swatch_step(frame: Frame) -> f32 {
    match frame {
        Frame::Full => 27.0,
        Frame::InGame => 25.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_full_saber_page_has_one_apply_as_retail_draws_it() {
        for items in [&SABER_FULL[..], &SABER_FULL_DUAL[..]] {
            let applies = items
                .iter()
                .filter(|item| matches!(item, Item::Apply | Item::ApplyMain))
                .count();
            assert_eq!(applies, 1);
        }
    }

    const PAGES: [ClassicPage; 5] = [
        ClassicPage::Player,
        ClassicPage::Character,
        ClassicPage::Saber,
        ClassicPage::Force,
        ClassicPage::Cosmetics,
    ];

    fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
        a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
    }

    #[test]
    fn every_entry_has_a_place_inside_its_window_and_none_overlap() {
        for page in PAGES {
            for frame in [Frame::Full, Frame::InGame] {
                for (dual, dark) in [(false, false), (true, false), (false, true)] {
                    let list = items(page, frame, dual, dark);
                    let [wx, wy, ww, wh] = window(page, frame);
                    for (index, item) in list.iter().enumerate() {
                        let r = rect(*item, page, frame, dual);
                        assert!(r[2] > 0.0 && r[3] > 0.0, "{page:?} {frame:?} {item:?}");
                        if on_canvas(*item, page, frame) {
                            assert!(r[0] >= 0.0 && r[1] >= 0.0);
                            assert!(r[0] + r[2] <= 640.0 && r[1] + r[3] <= 480.0);
                        } else {
                            assert!(r[0] >= wx && r[1] >= wy, "{page:?} {frame:?} {item:?}");
                            assert!(
                                r[0] + r[2] <= wx + ww + 0.5 && r[1] + r[3] <= wy + wh + 0.5,
                                "{page:?} {frame:?} {item:?}"
                            );
                        }
                        for other in &list[index + 1..] {
                            let o = rect(*other, page, frame, dual);
                            assert!(!overlaps(r, o), "{page:?} {frame:?}: {item:?} / {other:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn force_page_lists_every_power_of_its_side_once() {
        for (dark, side) in [(false, LIGHT_POWERS), (true, DARK_POWERS)] {
            for frame in [Frame::Full, Frame::InGame] {
                let list = items(ClassicPage::Force, frame, false, dark);
                let powers: Vec<usize> = list.iter().filter_map(|item| item.power()).collect();
                assert_eq!(powers.len(), 13);
                for power in NEUTRAL_POWERS.iter().chain(&SABER_POWERS).chain(&side) {
                    let count = powers.iter().filter(|p| **p == usize::from(*power)).count();
                    assert_eq!(count, 1);
                }
                assert_eq!(list.contains(&Item::Back), frame == Frame::Full);
                assert_eq!(list.contains(&Item::NavPlay), frame == Frame::Full);
            }
        }
        // Every power belongs to exactly one column, and has a hint.
        for index in 0..18_u8 {
            assert!(power_row(index).is_some());
            assert!(!Item::Power(index).hint().is_empty());
        }
        // The stars fit inside their row, right of the name.
        let row = force::row(force::LEFT, 138.0, 0);
        let first = force::star(row, 1);
        let last = force::star(row, 3);
        assert!(first[0] > row[0] + 100.0);
        assert!(last[0] + last[2] <= row[0] + row[2]);
    }

    #[test]
    fn grid_holds_the_retail_cells() {
        let [_, _, width, height] = rect(Item::Models, ClassicPage::Player, Frame::Full, false);
        assert_eq!((width / GRID_CELL) as usize, GRID_COLUMNS);
        assert_eq!((height / GRID_CELL) as usize, 3);
    }
}
