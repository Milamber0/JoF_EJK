//! Player identity for SJK: an Ed25519 key the player keeps, and the client side
//! of the SJK hub's protocol (`PROTOCOL.md` in Sol-Vulpes/SJK-hub) that turns it
//! into a profile and a badge on game servers.
//!
//! The crate has no window, renderer or game-network knowledge. The viewer
//! creates a [`Service`], feeds it the player's [`Settings`] and [`Location`],
//! and reads its [`Snapshot`].

#![warn(missing_docs)]

pub mod assets;
pub mod avatar;
pub mod bio;
pub mod chat;
pub mod crash;
pub mod donations;
pub mod emoji;
pub mod feed;
pub mod hub;
mod keys;
pub mod logs;
pub mod report;
pub mod service;
pub mod staff;
pub mod usage;
pub mod vitals;
pub mod wire;

pub use assets::Pack;
pub use crash::CrashReport;
pub use donations::DonationGoal;
pub use feed::{ChatState, ReceivedLooks};
pub use hub::{HttpHub, Hub, HubError, valid_base_url};
pub use keys::{Identity, KeyError};
pub use logs::{LogRequest, LogUpload};
pub use report::{
    BugReport, Category, NoteModel, NoteModelShader, NoteRemap, PlayerReport, WorldNote,
};
pub use service::{
    HolocronProgress, HubFactory, Location, ModelCheck, OpenOutcome, ReportOutcome, RevealOutcome,
    Service, Settings, Snapshot, Status,
};
pub use staff::{OpenedText, StaffAnswer, StaffQuery, StaffRequest, StaffState};
pub use vitals::{Sharing, VitalsTable};
pub use wire::{
    Achievement, CardStack, ChatMessage, DropEvent, DropMark, Emote, Feed, HOLOCRONS_LISTED,
    Holocron, HolocronCounts, HolocronState, Look, LookEvent, LootTier, Medal, OPEN_MAX,
    OpenAnswer, Opened, OpenedCard, OpenedUnlock, People, Person, Ping, Presence, Profile,
    RevealAnswer, Revealed, SharedVitals, Unlock, Vitals, WornName, names_match, normal_form,
};
