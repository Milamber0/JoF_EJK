//! `CG_PlayerSprites` (`codemp/cgame/cg_players.c`): the sprite cgame floats over a
//! player's head, the connection-trouble icon before the talk balloon.
//!
//! The voice-chat icon between them (`vChatTime`, from siege voice commands) is not
//! selected here; nothing in the client tracks voice-command events yet.

/// `EF_TALK` (`bg_public.h`): the player has a key catcher open.
const EF_TALK: u32 = sjk_game_jka::pmove_talk::EF_TALK;
/// `EF_CONNECTION` (`bg_public.h`): the server has had no command from the player
/// for a second (`ClientEndFrame`).
const EF_CONNECTION: u32 = 1 << 14;
/// `ET_NPC` (`bg_public.h` `entityType_t`).
const ET_NPC: u8 = crate::npc_identity::ET_NPC;

/// `CG_PlayerFloatSprite`: the sprite floats this far above the player's `lerpOrigin`.
pub const LEGACY_PLAYER_SPRITE_HEIGHT: f32 = 48.0;
/// `CG_PlayerFloatSprite`: the sprite's `radius`.
pub const LEGACY_PLAYER_SPRITE_RADIUS: f32 = 10.0;

/// One of the sprites `CG_PlayerSprites` floats over a player.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyPlayerSprite {
    /// `cgs.media.connectionShader`, for `EF_CONNECTION`.
    ConnectionInterrupted,
    /// `cgs.media.balloonShader`, for `EF_TALK`.
    Talk,
}

impl LegacyPlayerSprite {
    /// The shader `CG_RegisterGraphics` registers for this sprite (`cg_main.c`).
    pub const fn shader(self) -> &'static str {
        match self {
            Self::ConnectionInterrupted => "gfx/2d/net",
            Self::Talk => "gfx/mp/chat_icon",
        }
    }
}

/// Select the sprite stock floats over an entity drawn as a player: none while it
/// mind-tricks the local client, the connection icon for `EF_CONNECTION`, else the
/// talk balloon for `EF_TALK` on anything but an NPC.
pub fn legacy_player_sprite(
    entity_flags: u32,
    entity_type: u8,
    mind_tricked: bool,
) -> Option<LegacyPlayerSprite> {
    if mind_tricked {
        None
    } else if entity_flags & EF_CONNECTION != 0 {
        Some(LegacyPlayerSprite::ConnectionInterrupted)
    } else if entity_type != ET_NPC && entity_flags & EF_TALK != 0 {
        Some(LegacyPlayerSprite::Talk)
    } else {
        None
    }
}
