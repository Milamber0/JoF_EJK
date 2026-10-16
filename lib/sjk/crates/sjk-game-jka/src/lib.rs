//! Jedi Academy's game rules as a compatibility profile.
//!
//! One implementation of the `codemp` "both games" code (`bg_*.c`): the client
//! predicts with it and the server simulates with it, so the two cannot drift.
//! It sits between SJK's authoritative core, which knows no game, and the
//! protocol-26 adapter, which knows no rules. Its state is JKA-shaped because the
//! rules are; nothing here is an engine-wide constant.
//!
//! Sharing the code does not make a path server truth. A path becomes authoritative
//! only once a compiled reference oracle pins it at 8, 7, 4 and 3 ms command
//! spacing.

pub mod bot_chat;
pub mod bot_combat;
pub mod bot_ctf;
pub mod bot_destination;
pub mod bot_input;
pub mod bot_moves;
pub mod bot_objectives;
pub mod bot_personality;
pub mod bot_routes;
pub mod bot_senses;
pub mod bot_squad;
pub mod bot_standard;
pub mod bot_standard_fight;
pub mod bot_tactics;
pub mod bot_think;
pub mod bot_trail;
pub mod bot_weapons;
pub mod bots;
pub mod chat;
pub mod client_begin;
pub mod client_idle;
pub mod client_spawn;
pub mod client_view;
pub mod follow;
pub mod g2_player_angles;
pub mod game_log;
pub mod ghoul2_bolt;
pub mod icarus_set_table;
pub mod ip_filter;
pub mod means_of_death;
pub mod mover_team;
pub mod movers;
pub mod player_angle_flags;
pub mod player_angle_math;
pub mod ref_tags;
pub mod saber_clash;
pub mod saber_damage;
pub mod saber_rules;
mod saber_rules_table;
pub mod saber_splash;
pub mod script_calls;
pub mod script_entity;
pub mod script_gets;
pub mod script_host;
pub mod script_mover;
pub mod script_runner;
pub mod script_set;
mod script_set_npc;
mod script_set_tables;
pub mod server_skeleton;
pub mod tournament;
pub mod tri_tri;

pub mod client_timer;
pub mod concussion;
pub mod crt_rand;
pub mod ctf;
pub mod damage;
pub(crate) mod death_animation;
pub mod demp2;
pub mod disruptor;
pub mod duel;
pub mod emplaced;
pub mod entity_clip;
pub mod entity_id;
pub mod entity_pool;
pub mod event_entity;
pub mod force_config;
mod force_jump;
pub mod force_powers;
pub mod holocron;
pub(crate) mod item_physics;
pub mod items;
pub mod jedi_master;
pub mod knockdown;
pub mod melee;
pub mod mines;
pub mod npc_begin;
pub mod npc_mind;
pub mod npc_parms;
mod npc_parms_keys;
mod npc_parms_sabers;
pub mod npc_precache;
pub mod npc_roster;
pub mod npc_spawn;
pub mod npc_spawners;
pub mod player_death;
pub mod player_entity;
pub mod player_id;
pub mod pmove_emplaced;
pub mod pmove_hand_extend;
pub mod power_duel;
pub mod saber_definition;
pub mod saber_info;
pub mod script_world;
pub mod try_heal;

mod box_sweep;
pub mod map_turret;
pub mod map_turret_g2;
pub mod map_turret_model;
pub mod map_turret_world;
pub mod npc_senses;
pub mod npc_world;
pub mod vehicle;
pub mod vehicle_auto_board;
mod vehicle_blast;
pub mod vehicle_board;
pub mod vehicle_damage;
pub mod vehicle_death;
pub mod vehicle_drive;
mod vehicle_droid;
pub mod vehicle_fields;
pub mod vehicle_fighter;
pub mod vehicle_fighter_orient;
mod vehicle_gunnery;
pub mod vehicle_move;
pub mod vehicle_parms;
pub mod vehicle_predict;
pub mod vehicle_presentation;
pub mod vehicle_rider;
pub mod vehicle_riders;
mod vehicle_roster;
pub mod vehicle_ship_touch;
pub mod vehicle_skeleton;
pub mod vehicle_spawn;
mod vehicle_surfaces;
mod vehicle_think;
pub mod vehicle_triggers;
pub mod vehicle_turrets;
pub mod vehicle_update;
pub mod vehicle_walker;
pub mod vehicle_weapons;

pub mod nav_file;
pub mod npc_aim;
pub mod npc_atst;
pub mod npc_behavior;
pub mod npc_boba;
pub mod npc_client_think;
pub mod npc_combat;
pub mod npc_combat_points;
pub mod npc_commands;
pub mod npc_creature;
pub mod npc_damage;
pub mod npc_dead;
pub mod npc_death;
pub mod npc_dismember;
pub mod npc_dismember_check;
pub mod npc_droid;
mod npc_droid_head;
pub mod npc_enemy;
pub mod npc_force;
pub mod npc_force_throw;
pub mod npc_force_update;
pub mod npc_galak;
mod npc_galak_attack;
pub mod npc_grenadier;
pub mod npc_groups;
pub mod npc_howler;
pub mod npc_interrogator;
pub mod npc_jedi;
pub mod npc_jedi_block;
pub mod npc_jedi_combat;
pub mod npc_jedi_distance;
pub mod npc_jedi_evasion;
pub mod npc_jedi_glue;
pub mod npc_jedi_jump;
pub mod npc_jedi_moves;
pub mod npc_jedi_patrol;
pub mod npc_jedi_timers;
pub mod npc_machine;
pub mod npc_machine_parts;
pub mod npc_mark1;
pub mod npc_mark2;
pub mod npc_mine_monster;
pub mod npc_missile_block;
pub mod npc_nav;
mod npc_nav_ahead;
pub mod npc_nav_old;
pub mod npc_nav_route;
pub mod npc_nav_setup;
pub mod npc_nav_sources;
pub mod npc_navigator;
pub mod npc_pain;
pub mod npc_probe;
pub mod npc_rancor;
mod npc_rancor_attack;
pub mod npc_remote;
pub mod npc_saber;
pub mod npc_saber_bounce;
pub mod npc_saber_lock;
mod npc_saber_targets;
pub mod npc_saber_throw;
pub mod npc_sand_creature;
pub mod npc_seeker;
pub mod npc_sentry;
pub mod npc_skeleton;
pub mod npc_sniper;
pub mod npc_st;
pub mod npc_st_attack;
pub mod npc_st_commander;
pub mod npc_states;
pub mod npc_states_fight;
pub mod npc_states_flee;
pub mod npc_states_follow;
pub mod npc_states_script;
pub mod npc_states_search;
pub mod npc_think;
pub mod npc_touch;
pub mod npc_triggers;
pub mod npc_wampa;
mod npc_wampa_attack;
pub mod npc_weapon_pickup;

pub mod force_dark;
pub mod force_team;
pub mod force_throw;
pub mod force_trick;
pub mod player_sabers;
pub mod saber_drop;
mod saber_keywords;
pub mod saber_lock;
pub mod saber_stance;
pub mod saber_throw;
pub mod team_info;
pub mod text_parse;

pub mod breakables;
pub mod dropped_items;
pub mod generic_commands;
pub mod give;
pub mod noclip;
pub mod siege;
pub mod siege_class;
pub mod siege_items;
pub mod siege_map;
pub mod siege_text;
pub mod siege_triggers;
pub mod use_key;

pub mod intermission;
pub mod legacy_animation;
pub mod map;
pub mod match_end;
pub mod pmove;
pub mod pmove_anim;
pub mod pmove_debug_melee;
pub mod pmove_dir;
pub mod pmove_input_freeze;
pub mod pmove_japlus;
pub mod pmove_locomotion;
pub mod pmove_posture;
pub mod pmove_roll;
pub mod pmove_talk;
pub mod prediction_policy;
pub mod vote;

pub mod arenas;
pub mod fx_runner;
pub mod holdable_shield;
pub mod holdables;
pub mod map_logic;
pub mod map_scenery;
pub mod npc_names;
pub mod npc_rigid;
pub mod path_movers;
pub mod pmove_holdable;
pub(crate) mod pmove_lightsaber;
pub mod pmove_roll_anim;
pub mod pmove_roll_land;
pub mod pmove_rules;
pub mod pmove_saber;
pub(crate) mod pmove_saber_attack;
pub mod pmove_saber_lock;
pub(crate) mod pmove_saber_move;
pub(crate) mod pmove_slope;
pub mod pmove_speed;
pub mod pmove_weapon;
pub mod pmove_weapon_charge;
pub mod predicted_events;
pub mod prediction_items;
pub mod ranks;
pub mod registries;
pub mod saber_block;
pub mod saber_frame;
pub mod saber_move_data;
pub mod seeker_drone;
pub mod spawn_table;
pub mod target_speaker;
pub mod team_vote;
pub mod trajectory;
pub mod triggers;
pub mod userinfo;
pub mod warmup;
pub mod weapon_data;
pub mod weapon_fire;
pub mod world_effects;
pub mod worldspawn;

pub use intermission::{IntermissionView, PM_INTERMISSION, suppresses_movement};
pub use pmove_anim::{AnimationLengthTable, AnimationLengths, AnimationTiming};
pub use pmove_japlus::JaPlusRules;
pub use pmove_roll::{PMF_ROLLING, RollRules};
pub use sjk_protocol::UserCommand;
pub use trajectory::{
    legacy_evaluate_trajectory, legacy_evaluate_trajectory_angles, legacy_evaluate_trajectory_delta,
};
pub use weapon_data::{
    LEGACY_WEAPON_COUNT, LEGACY_WEAPON_DATA, LegacyWeaponData, legacy_weapon_data,
};

/// `pmtype_t::PM_SPECTATOR` (`bg_public.h:430`): flies, still runs into walls.
pub const PM_SPECTATOR: u8 = 4;

/// `EF_TELEPORT_BIT` (`bg_public.h`): toggled whenever an origin changes abruptly.
pub const EF_TELEPORT_BIT: u32 = 1 << 3;

/// Resolve one protocol animation ordinal to its `animation.cfg` token.
pub fn legacy_animation_name(index: usize) -> Option<&'static str> {
    legacy_animation::NAMES.get(index).copied()
}

/// The protocol ordinal of an `animation.cfg` token.
pub fn legacy_animation_index(name: &str) -> Option<usize> {
    legacy_animation::NAMES
        .iter()
        .position(|known| *known == name)
}

/// Number of animation ordinals in codemp's protocol lookup table.
pub fn legacy_animation_count() -> usize {
    legacy_animation::NAMES.len()
}
