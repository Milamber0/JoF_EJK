//! Compatibility primitives for the Jedi Academy network protocol.
//!
//! This crate intentionally contains no sockets. It turns legacy wire values
//! into typed engine data, keeping server-specific behavior out of the rest of
//! the engine.

mod adaptive_huffman;
mod byte_direction;
mod config_string_dirty;
mod demo;
mod dialect;
mod entity;
mod entity_write;
pub use config_string_dirty::ConfigStringDirty;
mod gamestate;
mod gamestate_write;
mod huffman_codes;
mod info_bytes;
mod info_string;
mod legacy_client_numbers;
mod legacy_entity_numbers;
mod legacy_text;
mod message;
mod snapshot;
mod usercmd;

pub use adaptive_huffman::{
    AdaptiveHuffmanError, compress_connect_block, decompress_connect_block,
};
pub use byte_direction::{legacy_byte_to_direction, legacy_direction_to_byte};
pub use demo::{DemoError, DemoReader, DemoRecord, MAX_LEGACY_MESSAGE_BYTES};
pub use dialect::{
    JaPlusCapabilities, JaProCapabilities, ServerDialect, ServerProfile, TaystJkCapabilities,
    is_ja_plus_game_name,
};
pub use entity::{
    ENTITY_NUMBER_NONE, EntityDeltaError, EntityField, EntityFieldEncoding, EntityState,
    LEGACY_ENTITY_FIELDS, LEGACY_ENTITY_NUMBER_BITS, MAX_LEGACY_ENTITIES, read_delta_entity,
};
pub use entity_write::{EntityWriteError, write_delta_entity};
pub use gamestate::{
    GameState, GameStateError, InitialGameStateMessage, MAX_BIG_INFO_STRING_BYTES,
    MAX_CONFIGSTRINGS, MAX_GAMESTATE_CHARS, decode_initial_gamestate,
    decode_initial_gamestate_with_schema,
};
pub use gamestate_write::{GameStateWriteError, write_gamestate_block, write_initial_gamestate};
pub use info_bytes::{info_pairs, info_remove_key, info_set_value, info_value};
pub use info_string::{InfoString, InfoStringError, set_value};
pub use legacy_client_numbers::{LEGACY_MAX_CLIENTS, LegacyClientNumbers};
pub use legacy_entity_numbers::{
    LEGACY_FIRST_GAME_ENTITY, LEGACY_GAME_ENTITIES, LEGACY_GAME_ENTITY_CEILING, LegacyEntityNumbers,
};
pub use legacy_text::{
    LegacyCharset, decode_legacy_text, encode_legacy_text, legacy_charset, looks_cyrillic,
    set_legacy_charset, windows_1251_byte, windows_1251_char, windows_1252_byte, windows_1252_char,
};
pub use message::{MessageError, MessageReader, MessageWriter, ServiceCommand};
pub use snapshot::write::{
    SnapshotHeader, SnapshotPlayer, SnapshotWriteError, SnapshotWriter, player_state_as_received,
};
pub use snapshot::{
    PlayerState, ReliableServerCommand, Snapshot, SnapshotError, VehicleNetFields,
    decode_base_snapshot, decode_snapshot,
};
pub use usercmd::{
    UserCommand, legacy_command_hash, read_delta_user_command, write_delta_user_command,
};
