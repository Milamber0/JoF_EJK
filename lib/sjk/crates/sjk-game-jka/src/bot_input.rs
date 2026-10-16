//! A bot's hands: botlib's elementary actions (OpenJK `codemp/botlib/be_ea.cpp`), which
//! the thinking fills, and `BotInputToUserCommand` (`codemp/game/ai_main.c`), which turns
//! them into the command a player would have sent.

use crate::player_death::Rng;
use sjk_protocol::UserCommand;

pub const ACTION_ATTACK: i32 = 0x1;
pub const ACTION_USE: i32 = 0x2;
pub const ACTION_RESPAWN: i32 = 0x8;
pub const ACTION_JUMP: i32 = 0x10;
pub const ACTION_MOVEUP: i32 = 0x20;
pub const ACTION_CROUCH: i32 = 0x80;
pub const ACTION_MOVEDOWN: i32 = 0x100;
pub const ACTION_MOVEFORWARD: i32 = 0x200;
pub const ACTION_MOVEBACK: i32 = 0x800;
pub const ACTION_MOVELEFT: i32 = 0x1000;
pub const ACTION_MOVERIGHT: i32 = 0x2000;
pub const ACTION_DELAYEDJUMP: i32 = 0x8000;
pub const ACTION_TALK: i32 = 0x1_0000;
pub const ACTION_GESTURE: i32 = 0x2_0000;
pub const ACTION_WALK: i32 = 0x8_0000;
pub const ACTION_FORCEPOWER: i32 = 0x10_0000;
pub const ACTION_ALT_ATTACK: i32 = 0x20_0000;
/// `be_ea.cpp`'s own: the bot jumped in its last think, so it cannot jump again yet.
const ACTION_JUMPEDLASTFRAME: i32 = 0x80_0000;
/// `MAX_USERMOVE`: the fastest a bot asks to move.
const MAX_USERMOVE: f32 = 400.0;

const BUTTON_ATTACK: u16 = 1;
const BUTTON_USE_HOLDABLE: u16 = 4;
const BUTTON_GESTURE: u16 = 8;
const BUTTON_WALKING: u16 = 16;
const BUTTON_USE: u16 = 32;
const BUTTON_ALT_ATTACK: u16 = 128;
const BUTTON_FORCEPOWER: u16 = 512;
/// `WP_BRYAR_PISTOL`: the weapon a bot that chose none asks for.
const WP_BRYAR_PISTOL: i32 = 4;

/// `bot_input_t`: what a bot asked for in its last think.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BotInput {
    /// Seconds since the last command, as `EA_GetInput` stamps it.
    pub thinktime: f32,
    /// Where to move, and how fast (up to 400).
    pub dir: [f32; 3],
    pub speed: f32,
    pub viewangles: [f32; 3],
    /// `ACTION_*`.
    pub actionflags: i32,
    pub weapon: i32,
}

impl BotInput {
    /// `EA_ResetInput`: a new think starts with nothing asked, remembering a jump.
    pub fn reset(&mut self) {
        let jumped = self.actionflags & ACTION_JUMP != 0;
        self.thinktime = 0.0;
        self.dir = [0.0; 3];
        self.speed = 0.0;
        self.actionflags = if jumped { ACTION_JUMPEDLASTFRAME } else { 0 };
    }

    /// `EA_Move`: the direction, the speed capped to ±400.
    pub fn move_towards(&mut self, dir: [f32; 3], speed: f32) {
        self.dir = dir;
        self.speed = speed.clamp(-MAX_USERMOVE, MAX_USERMOVE);
    }

    /// `EA_View`.
    pub fn view(&mut self, viewangles: [f32; 3]) {
        self.viewangles = viewangles;
    }

    /// `EA_SelectWeapon`.
    pub fn select_weapon(&mut self, weapon: i32) {
        self.weapon = weapon;
    }

    /// `EA_Attack`, `EA_Alt_Attack`, `EA_Use`, `EA_MoveForward` and the other plain
    /// actions: the flag set.
    pub fn act(&mut self, action: i32) {
        self.actionflags |= action;
    }

    /// `EA_Jump`: not twice in a row.
    pub fn jump(&mut self) {
        if self.actionflags & ACTION_JUMPEDLASTFRAME != 0 {
            self.actionflags &= !ACTION_JUMP;
        } else {
            self.actionflags |= ACTION_JUMP;
        }
    }

    /// `EA_DelayedJump`: a jump the next command makes, not twice in a row.
    pub fn delayed_jump(&mut self) {
        if self.actionflags & ACTION_JUMPEDLASTFRAME != 0 {
            self.actionflags &= !ACTION_DELAYEDJUMP;
        } else {
            self.actionflags |= ACTION_DELAYEDJUMP;
        }
    }
}

/// `BotInputToUserCommand`: `input` as the command for `time`, its angles less
/// `delta_angles`, its movement made relative to the view; with `use_time` in the past,
/// the use button pressed at random (`Q_irand(1, 10) < 5`) against `level_time`.
pub fn input_to_user_command(
    input: &mut BotInput,
    delta_angles: [i32; 3],
    time: i32,
    use_time: i32,
    level_time: i32,
    rng: &mut Rng,
) -> UserCommand {
    let mut command = UserCommand {
        server_time: time,
        ..UserCommand::default()
    };
    if input.actionflags & ACTION_DELAYEDJUMP != 0 {
        input.actionflags = (input.actionflags | ACTION_JUMP) & !ACTION_DELAYEDJUMP;
    }
    let flags = input.actionflags;
    if flags & ACTION_RESPAWN != 0 {
        command.buttons = BUTTON_ATTACK;
    }
    for (action, button) in [
        (ACTION_ATTACK, BUTTON_ATTACK),
        (ACTION_ALT_ATTACK, BUTTON_ALT_ATTACK),
        (ACTION_GESTURE, BUTTON_GESTURE),
        (ACTION_USE, BUTTON_USE_HOLDABLE),
        (ACTION_WALK, BUTTON_WALKING),
        (ACTION_FORCEPOWER, BUTTON_FORCEPOWER),
    ] {
        if flags & action != 0 {
            command.buttons |= button;
        }
    }
    if use_time < level_time && rng.irand(1, 10) < 5 {
        command.buttons |= BUTTON_USE;
    }
    if input.weapon == 0 {
        input.weapon = WP_BRYAR_PISTOL;
    }
    command.weapon = input.weapon as u8;
    for axis in 0..3 {
        // `ANGLE2SHORT`, less the delta, kept as a short.
        let short = ((input.viewangles[axis] * 65_536.0 / 360.0) as i32) & 65_535;
        command.angles[axis] = i32::from(short.wrapping_sub(delta_angles[axis]) as i16);
    }
    let pitch = if input.dir[2] != 0.0 {
        input.viewangles[0]
    } else {
        0.0
    };
    let (forward, right) = crate::pmove::flight::flight_axes([pitch, input.viewangles[1], 0.0]);
    let (forward, right) = (forward.to_array(), right.to_array());
    input.speed = input.speed * 127.0 / 400.0;
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (mut f, mut r, mut u) = (
        dot(forward, input.dir),
        dot(right, input.dir),
        forward[2].abs() * input.dir[2],
    );
    let most = f.abs().max(r.abs()).max(u.abs());
    if most > 0.0 {
        f *= input.speed / most;
        r *= input.speed / most;
        u *= input.speed / most;
    }
    // Float to `signed char`: towards zero.
    (command.forward_move, command.right_move, command.up_move) = (f as i8, r as i8, u as i8);
    if flags & ACTION_MOVEFORWARD != 0 {
        command.forward_move = 127;
    }
    if flags & ACTION_MOVEBACK != 0 {
        command.forward_move = -127;
    }
    if flags & ACTION_MOVELEFT != 0 {
        command.right_move = -127;
    }
    if flags & ACTION_MOVERIGHT != 0 {
        command.right_move = 127;
    }
    if input.actionflags & ACTION_JUMP != 0 {
        command.up_move = 127;
    }
    if flags & ACTION_CROUCH != 0 {
        command.up_move = -127;
    }
    command
}
