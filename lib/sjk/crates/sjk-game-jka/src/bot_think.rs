//! A bot's thinking each frame (OpenJK `codemp/game/ai_main.c`): `BotAIStartFrame`'s
//! clock and variable refresh, `BotAI` (the bot's state read, `StandardBotAI` run with
//! the delta angles added), `BotChangeViewAngles` and `BotUpdateInput`, whose command the
//! server runs as a player's (`BotUserCommand`).
//!
//! `StandardBotAI`'s opening is here: a spectating bot drops its trail and does nothing,
//! and the `bot_forgimmick` modes act; its body is [`crate::bot_standard`].

use crate::bot_input::{
    ACTION_ATTACK, ACTION_MOVEBACK, ACTION_MOVEFORWARD, ACTION_MOVELEFT, ACTION_MOVERIGHT,
    BotInput, input_to_user_command,
};
use crate::bot_personality::BotSkills;
use crate::player_angle_math::{angle_mod, normalize};
use crate::player_death::Rng;
use sjk_protocol::UserCommand;

/// `TEAM_SPECTATOR`.
const TEAM_SPECTATOR: i32 = 3;
/// `BUTTON_ATTACK`.
const BUTTON_ATTACK: u16 = 1;
/// `ACTION_RESPAWN`.
const ACTION_RESPAWN: i32 = 0x8;

/// `SHORT2ANGLE`.
fn short_to_angle(short: i32) -> f32 {
    short as f32 * (360.0 / 65_536.0)
}

/// `AngleDifference` (`ai_main.c`): `ang1 - ang2` brought within ±180 on the side
/// the larger one is.
pub(crate) fn angle_difference(first: f32, second: f32) -> f32 {
    let mut difference = first - second;
    if first > second {
        if difference > 180.0 {
            difference = (f64::from(difference) - 360.0) as f32;
        }
    } else if difference < -180.0 {
        difference = (f64::from(difference) + 360.0) as f32;
    }
    difference
}

/// The variables a bot frame reads, refreshed once a second (`gUpdateVars`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BotVariables {
    /// `bot_forgimmick`: 0, or a test mode that replaces the thinking.
    pub forgimmick: i32,
}

/// `BotAIStartFrame`'s clock: the last frame's time, when the variables are next read,
/// and what they were.
#[derive(Clone, Copy, Debug, Default)]
pub struct BotClock {
    local_time: i32,
    update_vars_at: i32,
    pub variables: BotVariables,
}

/// One bot frame's times: the milliseconds since the last, and each think's length.
#[derive(Clone, Copy, Debug)]
pub struct BotFrameTime {
    pub time: i32,
    pub elapsed: i32,
    pub thinktime: i32,
}

impl BotClock {
    /// `BotAIStartFrame`'s opening at `time`: the variables read (through `read`) when
    /// the last reading is a second old at `level_time`, the elapsed time taken.
    /// `BOT_THINK_TIME` is 0, so every bot thinks every frame, for the whole elapsed time.
    pub fn start_frame(
        &mut self,
        time: i32,
        level_time: i32,
        read: impl FnOnce() -> BotVariables,
    ) -> BotFrameTime {
        if self.update_vars_at < level_time {
            self.variables = read();
            self.update_vars_at = level_time + 1000;
        }
        let elapsed = time - self.local_time;
        self.local_time = time;
        BotFrameTime {
            time,
            elapsed,
            thinktime: elapsed.max(0),
        }
    }
}

/// What a bot reads of the game as it thinks (`BotAI_GetClientState`, `g_entities`).
#[derive(Clone, Copy, Debug)]
pub struct BotView {
    pub origin: [f32; 3],
    pub viewheight: i32,
    pub delta_angles: [i32; 3],
    pub team: i32,
    /// Client 0's origin, where there is a client 0 in the game (mode 4's target).
    pub first_client: Option<[f32; 3]>,
}

/// `bot_state_t`, as far as the thinking so far uses it.
#[derive(Clone, Debug, Default)]
pub struct BotMind {
    /// `settings.skill`.
    pub skill: f32,
    pub skills: BotSkills,
    pub viewangles: [f32; 3],
    pub ideal_viewangles: [f32; 3],
    pub viewanglespeed: [f32; 3],
    /// Seconds the bot has thought, in total.
    pub ltime: f32,
    pub thinktime: f32,
    pub origin: [f32; 3],
    pub eye: [f32; 3],
    /// `cur_ps.delta_angles` as the last think read them.
    pub delta_angles: [i32; 3],
    /// `botthink_residual`.
    pub think_residual: i32,
    /// `lastucmd`: the command last sent.
    pub last_command: UserCommand,
    /// `noUseTime`: until when the use button is not pressed at random.
    pub no_use_time: i32,
    /// `virtualWeapon`: the weapon last asked for, which the choice does not ask again.
    pub virtual_weapon: i32,
    /// `forceMove_Forward`, `forceMove_Right`, `forceMove_Up`: moves the test modes make.
    pub force_move: [i32; 3],
    /// Its elementary actions (`botinputs[client]`).
    pub input: BotInput,
    /// The trail (`crate::bot_trail`): the point it heads for, where it wants to get,
    /// and which way along the trail it goes (0 forward, 1 back).
    pub wp_current: Option<usize>,
    pub wp_destination: Option<usize>,
    pub wp_direction: i32,
    /// Where it camps and looks from (`wpCamping`, `wpCampingTo`), until when
    /// (`isCamping`), standing or crouched.
    pub wp_camping: Option<usize>,
    pub wp_camping_to: Option<usize>,
    pub is_camping: f32,
    pub camp_standing: bool,
    /// The trail's clocks, floats as the reference keeps them.
    pub wp_switch_time: f32,
    /// `wpDestSwitchTime`: until when its destination stays.
    pub wp_dest_switch_time: f32,
    pub wp_seen_time: f32,
    pub wp_travel_time: f32,
    pub destination_grab_time: f32,
    pub chicken_wuss_calculation_time: f32,
    /// Moving on: stand still, crouch, jump, prepare a jump, charge and make a Force jump.
    pub be_still: f32,
    pub duck_time: f32,
    pub jump_time: f32,
    pub jump_prep: f32,
    pub force_jumping: f32,
    pub force_jump_charge_time: i32,
    /// Wandering instead of going for goals (`randomNav`), until when it is redrawn.
    pub random_nav: i32,
    pub random_nav_time: i32,
    /// `runningLikeASissy`: running from something just now.
    pub running_like_a_sissy: i32,
    /// From its personality: a camper (2 always camps) and a saber specialist.
    pub is_camper: i32,
    pub saber_specialist: i32,
    /// `ctfState`: its part in a flag game.
    pub ctf_state: i32,
    /// `siegeState`: attacking or defending in siege; `jmState`: who has the Jedi
    /// Master's saber (-1 nobody).
    pub siege_state: i32,
    pub jm_state: i32,
    /// Its squad: whether it leads one, whom it follows (`squadLeader`), a role a leader
    /// forced on it (`state_Forced`), its teamplay role, and the squad's clocks.
    pub is_squad_leader: i32,
    pub squad_leader: Option<i32>,
    pub state_forced: i32,
    pub teamplay_state: i32,
    pub squad_regroup_interval: i32,
    pub squad_cannot_lead: i32,
    /// Its fighting: attack and alternate-fire this frame, how long it charges the
    /// alternate fire, the strafe's clock and side, saber defence and its clock, backing
    /// off (`saberBFTime`, `saberBTime`), pausing (`saberSTime`), its det pack's plant
    /// and the order to set it off.
    pub do_attack: i32,
    pub do_alt_attack: i32,
    pub alt_charge_time: i32,
    pub melee_strafe_time: f32,
    pub melee_strafe_dir: i32,
    pub saber_defending: i32,
    pub saber_defend_decide_time: i32,
    pub saber_bf_time: i32,
    pub saber_b_time: i32,
    pub saber_s_time: i32,
    pub plant_continue: i32,
    pub plant_kill_em_all: i32,
    /// An objective's entity to shoot (`shootGoal`) or to touch (`touchGoal`).
    pub shoot_goal: Option<i32>,
    pub touch_goal: Option<i32>,
    /// `frame_Enemy_Len`: how far its enemy was this frame.
    pub frame_enemy_len: f32,
    /// Its enemy's entity number (`currentEnemy`), until when it remembers it unseen.
    pub current_enemy: Option<i32>,
    pub enemy_seen_time: f32,
    /// `doForcePush`: until when it pushes a missile away.
    pub do_force_push: i32,
    /// `dontGoBack`: until when it keeps running from a danger.
    pub dont_go_back: f32,
    /// Getting away: from which danger (`dangerousObject`), until when the direction holds
    /// (`escapeDirTime`), the destination set aside meanwhile (`wpStoreDest`), until when
    /// destinations are ignored, and whether it runs from a threat.
    pub dangerous_object: Option<i32>,
    pub escape_dir_time: f32,
    pub wp_store_dest: Option<usize>,
    pub wp_dest_ignore_time: f32,
    pub running_to_escape_threat: i32,
    /// `doingFallback`: lost off the trail.
    pub doing_fallback: bool,
    /// Its chat: whether and how often it chats (`canChat`, `chatFrequency`), its chat
    /// groups (`gBotChatBuffer`), a line waiting (`doChat`, 2 for a greeting) and when
    /// it goes out, to its team or not, whom it names, the line itself (the whole
    /// buffer, as lines are written over one another), and how far its hatred goes
    /// (`loved_death_thresh`), who last hurt it (`lastHurt`).
    pub can_chat: i32,
    pub chat_frequency: i32,
    pub chat_buffer: Vec<u8>,
    pub do_chat: i32,
    pub chat_time: f32,
    pub chat_time_stored: f32,
    pub chat_team: i32,
    pub chat_object: Option<i32>,
    pub chat_alt_object: Option<i32>,
    pub current_chat: Vec<u8>,
    pub loved_death_thresh: i32,
    pub last_hurt: Option<i32>,
    /// The bots it loves (`loved`), from its personality.
    pub loved: Vec<crate::bot_personality::BotAttachment>,
    /// `frame_Enemy_Vis`: it sees its enemy this frame.
    pub frame_enemy_vis: bool,
    /// `frame_Waypoint_Len`: how far its point was this frame.
    pub frame_waypoint_len: f32,
    /// Where it wants to look (`goalAngles`) and to go (`goalPosition`).
    pub goal_angles: [f32; 3],
    pub goal_position: [f32; 3],
    /// Its aim's wobble: until when, and by how much.
    pub aim_offset_time: f32,
    pub aim_offset_amt_yaw: f32,
    pub aim_offset_amt_pitch: f32,
    /// The one it hates (`revengeEnemy`) and how much.
    pub revenge_enemy: Option<i32>,
    pub revenge_hate_level: i32,
    /// `meleeStrafeDisable`: until when it does not strafe.
    pub melee_strafe_disable: f32,
    /// `lastAttacked`: the client it last hurt, while no other has hurt that one since.
    pub last_attacked: Option<i32>,
    /// `staticFlagSpot`: where the dropped flag it went for lay.
    pub static_flag_spot: [f32; 3],
    /// `lastDeadTime`: when it was last dead (or spawned); `deathActivitiesDone`: its
    /// death was mourned.
    pub last_dead_time: i32,
    pub death_activities_done: bool,
    /// `timeToReact`: when it may first fight an enemy it has just found.
    pub time_to_react: f32,
    /// Where it last saw its enemy (`lastEnemySpotted`), from where
    /// (`hereWhenSpotted`), whom (`lastVisibleEnemyIndex`, `ENTITYNUM_NONE` for none),
    /// and `hitSpotted`.
    pub last_enemy_spotted: [f32; 3],
    pub here_when_spotted: [f32; 3],
    pub last_visible_enemy_index: i32,
    pub hit_spotted: i32,
    /// `jumpHoldTime`: until when the jump is held; `jDelay`: jumps wait until then.
    pub jump_hold_time: f32,
    pub j_delay: f32,
    /// `frame_Waypoint_Vis`: it sees its point this frame.
    pub frame_waypoint_vis: bool,
    /// `goalMovedir`: the way it last moved.
    pub goal_movedir: [f32; 3],
    /// `forceWeaponSelect`: a weapon it must hold (a mine or a det pack to plant, a
    /// det pack to set off); its planting's clocks (`plantTime`, `plantDecided`).
    pub force_weapon_select: i32,
    pub plant_time: i32,
    pub plant_decided: i32,
    /// Its saber: when it may throw it next, and whether and until when it favours the
    /// strong style.
    pub saber_throw_time: i32,
    pub saber_power: bool,
    pub saber_power_time: i32,
    /// `botChallengingTime`: until when it holds still, answering a duel challenge.
    pub bot_challenging_time: i32,
}

impl BotMind {
    /// `BotAISetupClient`: a new bot at `skill` with its personality's skills.
    pub fn new(skill: f32, skills: BotSkills) -> Self {
        Self {
            skill,
            skills,
            ..Self::default()
        }
    }

    /// `BotResetState` (a map restart) and `BotAISetupClient` again (a new map): the
    /// thinking starts over; its settings, the delta angles it last read and botlib's
    /// input stay.
    pub fn reset(&mut self) {
        *self = Self {
            skill: self.skill,
            skills: self.skills,
            delta_angles: self.delta_angles,
            input: self.input,
            ..Self::default()
        };
    }

    /// Whether the bot thinks this frame (`botthink_residual`); every bot in use counts
    /// the time, connected or not.
    pub fn due(&mut self, frame: &BotFrameTime) -> bool {
        self.think_residual += frame.elapsed;
        if self.think_residual >= frame.thinktime {
            self.think_residual -= frame.thinktime;
            return true;
        }
        false
    }

    /// `BotAI` without the body: the input cleared, the state read, `StandardBotAI`'s
    /// opening with the delta angles added to the view, which [`Self::end_think`] takes
    /// off again.
    pub fn think(&mut self, view: &BotView, frame: &BotFrameTime, variables: &BotVariables) {
        self.begin_think(view, frame, variables);
        self.end_think();
    }

    /// `BotAI` up to `StandardBotAI`'s body: the input cleared, the state read, the delta
    /// angles added, the opening run. Whether the body
    /// ([`crate::bot_standard::standard_bot_ai`]) runs next — not for a spectator or a
    /// test mode.
    pub fn begin_think(
        &mut self,
        view: &BotView,
        frame: &BotFrameTime,
        variables: &BotVariables,
    ) -> bool {
        self.input.reset();
        self.delta_angles = view.delta_angles;
        self.add_delta_angles(1.0);
        let thinktime = frame.thinktime as f32 / 1000.0;
        self.ltime += thinktime;
        self.thinktime = thinktime;
        self.origin = view.origin;
        self.eye = view.origin;
        self.eye[2] += view.viewheight as f32;
        self.standard(view, variables)
    }

    /// `BotAI`'s end: the delta angles taken off the view.
    pub fn end_think(&mut self) {
        self.add_delta_angles(-1.0);
    }

    fn add_delta_angles(&mut self, sign: f32) {
        for axis in 0..3 {
            self.viewangles[axis] =
                angle_mod(self.viewangles[axis] + sign * short_to_angle(self.delta_angles[axis]));
        }
    }

    /// `StandardBotAI`'s opening: nothing for a spectator; the `bot_forgimmick` modes.
    /// Whether the body runs.
    fn standard(&mut self, view: &BotView, variables: &BotVariables) -> bool {
        if view.team == TEAM_SPECTATOR {
            self.drop_trail();
            return false;
        }
        if variables.forgimmick == 0 {
            return true;
        }
        self.drop_trail();
        match variables.forgimmick {
            2 => self.input.act(ACTION_ATTACK),
            3 => {
                let mut direction = self.origin;
                normalize(&mut direction);
                self.input.act(ACTION_ATTACK);
                self.input.move_towards(direction, 5000.0);
            }
            4 => {
                if let Some(target) = view.first_client {
                    let mut direction = [
                        target[0] - self.origin[0],
                        target[1] - self.origin[1],
                        target[2] - self.origin[2],
                    ];
                    normalize(&mut direction);
                    self.input.move_towards(direction, 5000.0);
                }
            }
            _ => {}
        }
        let [forward, right, up] = self.force_move;
        if forward != 0 {
            self.input.act(if forward > 0 {
                ACTION_MOVEFORWARD
            } else {
                ACTION_MOVEBACK
            });
        }
        if right != 0 {
            self.input.act(if right > 0 {
                ACTION_MOVERIGHT
            } else {
                ACTION_MOVELEFT
            });
        }
        if up != 0 {
            self.input.jump();
        }
        false
    }

    /// A spectator's frame and the test modes' (`StandardBotAI`'s openings): no point,
    /// enemy or destination, the trail forward.
    pub fn drop_trail(&mut self) {
        self.wp_current = None;
        self.current_enemy = None;
        self.wp_destination = None;
        self.wp_direction = 0;
    }

    /// `BotChangeViewAngles`: the view turned towards the ideal angles at the bot's turn
    /// speed (its combat speed by skill while it sees an enemy), at most `maxturn`
    /// degrees a second, then handed to the input (`EA_View`).
    fn change_view_angles(&mut self, thinktime: f32) {
        if self.ideal_viewangles[0] > 180.0 {
            self.ideal_viewangles[0] -= 360.0;
        }
        let mut factor = self.skills.turnspeed;
        if factor > 1.0 {
            factor = 1.0;
        }
        if f64::from(factor) < 0.001 {
            factor = 0.001;
        }
        let maxchange = self.skills.maxturn * thinktime;
        for axis in 0..2 {
            self.viewangles[axis] = angle_mod(self.viewangles[axis]);
            self.ideal_viewangles[axis] = angle_mod(self.ideal_viewangles[axis]);
            let difference = angle_difference(self.viewangles[axis], self.ideal_viewangles[axis]);
            let desired = difference * factor;
            let speed = &mut self.viewanglespeed[axis];
            *speed += *speed - desired;
            if *speed > 180.0 {
                *speed = maxchange;
            }
            if *speed < -180.0 {
                *speed = -maxchange;
            }
            let mut turn = *speed;
            if turn > maxchange {
                turn = maxchange;
            }
            if turn < -maxchange {
                turn = -maxchange;
            }
            self.viewangles[axis] = angle_mod(self.viewangles[axis] + turn);
            *speed = (f64::from(*speed) * (0.45 * f64::from(1.0 - factor))) as f32;
        }
        if self.viewangles[0] > 180.0 {
            self.viewangles[0] -= 360.0;
        }
        self.input.view(self.viewangles);
    }

    /// `BotUpdateInput` at `time`: the view turned, the input read back (`EA_GetInput`)
    /// and made the command (`BotInputToUserCommand`) against `level_time`, which the
    /// server runs for the bot (`BotUserCommand`).
    pub fn update_input(
        &mut self,
        frame: &BotFrameTime,
        level_time: i32,
        rng: &mut Rng,
    ) -> UserCommand {
        self.add_delta_angles(1.0);
        self.change_view_angles(frame.elapsed as f32 / 1000.0);
        self.input.thinktime = frame.time as f32 / 1000.0;
        let mut input = self.input;
        // The respawn hack: not a second press while the last command held the button.
        if input.actionflags & ACTION_RESPAWN != 0 && self.last_command.buttons & BUTTON_ATTACK != 0
        {
            input.actionflags &= !(ACTION_RESPAWN | ACTION_ATTACK);
        }
        self.last_command = input_to_user_command(
            &mut input,
            self.delta_angles,
            frame.time,
            self.no_use_time,
            level_time,
            rng,
        );
        self.add_delta_angles(-1.0);
        self.last_command
    }
}
