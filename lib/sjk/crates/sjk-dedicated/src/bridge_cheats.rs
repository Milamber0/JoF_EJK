//! The cheats of a server with `sv_cheats` that live here: `t_use name`
//! (`Cmd_TargetUse_f`, `g_cmds.c`), which fires a name as the player
//! (`G_UseTargets2(ent, ent, name)`), as mappers test a map's targets and scripts; and
//! `noclip` (`Cmd_Noclip_f`), which lets the player fly through the world
//! ([`sjk_game_jka::noclip`]; the think applies it). (`setviewpos` is
//! `bridge_teleport`'s, `give` `bridge_commands`'.)

use super::*;

impl NativeGame {
    /// `ClientCommand` for `t_use` and `noclip`: whether `text` was one of them.
    pub(super) fn cheat_command(&mut self, client: usize, text: &[u8]) -> bool {
        let mut words = text
            .split(|byte| byte.is_ascii_whitespace())
            .filter(|word| !word.is_empty());
        let Some(command) = words.next() else {
            return false;
        };
        let noclip = command.eq_ignore_ascii_case(b"noclip");
        if !noclip && !command.eq_ignore_ascii_case(b"t_use") {
            return false;
        }
        // `ClientCommand`'s gates (`g_cmds.c:3466-3480`), in its order: `noclip` is
        // also `CMD_NOINTERMISSION`.
        if noclip && self.refused_at_intermission(client, b"noclip") {
            return true;
        }
        if !self.settings.cheats {
            self.told
                .push(Told::One(client, b"print \"@@@NOCHEATS\n\"".to_vec()));
            return true;
        }
        if self
            .peer(client)
            .is_none_or(|peer| peer.health <= 0 || !peer.playing())
        {
            self.told
                .push(Told::One(client, b"print \"@@@MUSTBEALIVE\n\"".to_vec()));
            return true;
        }
        if noclip {
            // `Cmd_Noclip_f`: the flag toggled and the player told; its next think puts
            // it in (or takes it out of) `PM_NOCLIP`.
            if let Some(peer) = self.peer_mut(client) {
                let message = sjk_game_jka::noclip::toggle(&mut peer.noclip);
                self.told.push(Told::One(client, message.to_vec()));
            }
            return true;
        }
        // `Cmd_TargetUse_f`: needs a name.
        if let Some(name) = words.next() {
            let name = String::from_utf8_lossy(name).into_owned();
            self.fire_targets(&name, client, self.last_frame_time);
        }
        true
    }
}
