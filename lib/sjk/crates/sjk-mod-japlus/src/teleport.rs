//! Teleports through JA+'s `amtele`, from JoF EternalJK's `teleGun`, `get`, `goto`,
//! `amTeleOffset`, `PTelemark` and `PTele` (`codemp/cgame/cg_consolecmds.c:88-162,
//! 2570-2696`). The client only works out where; the server checks the admin rights.

use sjk_mod::{Host, Me, Player, forward, number, resolve_player};

/// How far above the floor a teleport lands, as JA+'s own teleports do.
const LIFT: f32 = 24.0;
/// `get` and `goto` put the player this far in front by default.
const DISTANCE: f32 = 100.0;
/// `goto` trusts a drawn position for this long before asking the server.
const STALE_MS: i32 = 1_000;

fn me(host: &dyn Host) -> Result<Me, String> {
    host.me().ok_or_else(|| "not in a game".to_owned())
}

fn add(origin: [f32; 3], direction: [f32; 3], distance: f32) -> [f32; 3] {
    std::array::from_fn(|axis| origin[axis] + direction[axis] * distance)
}

fn tele(position: [f32; 3], yaw: f32) -> String {
    format!(
        "amtele {:.6} {:.6} {:.6} {:.6}",
        position[0], position[1], position[2], yaw
    )
}

/// `japlus.guntele [distance] [yaw offset]`: to the crosshair's point, or
/// `distance` along the view.
pub(crate) fn gun_tele(
    args: &[String],
    me: Me,
    crosshair: Option<[f32; 3]>,
) -> Result<String, String> {
    let yaw = me.view_angles[1];
    let Some(distance) = args.first() else {
        let point = crosshair
            .filter(|point| *point != [0.0; 3])
            .ok_or("aim at a surface first")?;
        return Ok(tele([point[0], point[1], point[2] + LIFT], yaw));
    };
    let offset = args.get(1).map_or(0.0, |text| number(text));
    let position = add(me.origin, forward(me.view_angles), number(distance));
    Ok(tele(position, yaw + offset))
}

/// The player `get`/`goto` act on: the crosshair's without a name or with `gun`.
fn target(args: &[String], host: &dyn Host, players: &[Player]) -> Result<u16, String> {
    match args.first() {
        Some(query) if !query.eq_ignore_ascii_case("gun") => resolve_player(players, query),
        _ => host
            .crosshair_player()
            .ok_or_else(|| "aim at a player, or name one".to_owned()),
    }
}

/// The distance and yaw offset after the player argument.
fn placement(args: &[String]) -> (f32, f32) {
    (
        args.get(1).map_or(DISTANCE, |text| number(text)),
        args.get(2).map_or(0.0, |text| number(text)),
    )
}

/// `japlus.bring [id|name|gun] [distance] [yaw offset]`: the player to in front
/// of you, facing you.
pub(crate) fn bring(args: &[String], host: &dyn Host) -> Result<String, String> {
    let me = me(host)?;
    let client = target(args, host, &host.players())?;
    let (distance, offset) = placement(args);
    let mut angles = me.view_angles;
    // Without a distance the player lands level with you, lifted off the floor.
    let level = args.len() <= 1;
    if level {
        angles[0] = 0.0;
        angles[2] = 0.0;
    }
    let mut position = add(me.origin, forward(angles), distance);
    if level {
        position[2] = me.origin[2] + LIFT;
    }
    Ok(format!(
        "amtele {client} {:.6} {:.6} {:.6} {:.6}",
        position[0],
        position[1],
        position[2],
        me.view_angles[1] + 180.0 + offset
    ))
}

/// `japlus.goto [id|name|gun] [distance] [yaw offset]`: you to in front of the
/// player, facing it; to the player itself when it is not drawn.
pub(crate) fn go_to(args: &[String], host: &dyn Host) -> Result<(String, Option<String>), String> {
    me(host)?;
    let players = host.players();
    let client = target(args, host, &players)?;
    let drawn = players
        .iter()
        .find(|player| player.client == client)
        .and_then(|player| player.drawn)
        .filter(|drawn| drawn.age_ms < STALE_MS);
    let Some(drawn) = drawn else {
        return Ok((
            format!("amtele {client}"),
            Some("^3Player not in view; teleporting to it through the server".into()),
        ));
    };
    let (distance, offset) = placement(args);
    let mut angles = drawn.angles;
    if args.len() <= 1 {
        angles[0] = 0.0;
        angles[2] = 0.0;
    }
    let mut position = add(drawn.origin, forward(angles), distance);
    if args.len() == 1 {
        position[2] = drawn.base_z + LIFT;
    }
    Ok((tele(position, drawn.angles[1] + 180.0 + offset), None))
}

/// `japlus.teleoffset x [y [z]]`: by whole units from where you are.
pub(crate) fn offset(args: &[String], me: Me) -> Result<String, String> {
    if args.is_empty() || args.len() > 3 {
        return Err("usage: japlus.teleoffset x [y [z]]".into());
    }
    let mut position = me.origin;
    for (axis, text) in args.iter().enumerate() {
        position[axis] += number(text).trunc();
    }
    Ok(tele(position, me.view_angles[1]))
}

/// A position kept by `japlus.mark`, in whole units as `PTelemark` keeps it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Mark([i32; 4]);

impl Mark {
    pub(crate) fn of(me: Me) -> Self {
        let [x, y, z] = me.origin.map(|value| value as i32);
        Self([x, y, z, me.view_angles[1] as i32])
    }

    pub(crate) fn describe(self) -> String {
        let [x, y, z, yaw] = self.0;
        format!("Mark set ({x} {y} {z}) : {yaw}")
    }

    /// The server command `japlus.recall` sends.
    pub(crate) fn recall(self) -> String {
        let [x, y, z, yaw] = self.0;
        format!("setviewpos {x} {y} {z} {yaw}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_mod::{Drawn, Server};

    fn me() -> Me {
        Me {
            client: 0,
            origin: [100.0, 200.0, 24.0],
            view_angles: [30.0, 90.0, 0.0],
            intermission: false,
        }
    }

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    struct Fake {
        crosshair: Option<u16>,
        players: Vec<Player>,
    }

    impl Host for Fake {
        fn send(&mut self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn server(&self) -> Option<Server> {
            None
        }
        fn me(&self) -> Option<Me> {
            Some(me())
        }
        fn crosshair_point(&self) -> Option<[f32; 3]> {
            None
        }
        fn crosshair_player(&self) -> Option<u16> {
            self.crosshair
        }
        fn players(&self) -> Vec<Player> {
            self.players.clone()
        }
        fn cvar(&self, _: &str) -> Option<String> {
            None
        }
        fn set_cvar(&mut self, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
    }

    fn fake(age_ms: i32) -> Fake {
        Fake {
            crosshair: Some(4),
            players: vec![Player {
                client: 4,
                name: "Kyle".into(),
                drawn: Some(Drawn {
                    origin: [0.0, 0.0, 40.0],
                    angles: [10.0, 0.0, 0.0],
                    base_z: 32.0,
                    age_ms,
                }),
            }],
        }
    }

    #[test]
    fn gun_tele_lands_on_the_crosshair_point_or_along_the_view() {
        assert_eq!(
            gun_tele(&[], me(), Some([1.0, 2.0, 3.0])).unwrap(),
            "amtele 1.000000 2.000000 27.000000 90.000000"
        );
        assert!(gun_tele(&[], me(), Some([0.0; 3])).is_err());
        assert!(gun_tele(&[], me(), None).is_err());
        // 100 units along a view 30 degrees down, turned 45 degrees.
        assert_eq!(
            gun_tele(&args("100 45"), me(), None).unwrap(),
            "amtele 100.000000 286.602539 -26.000000 135.000000"
        );
    }

    #[test]
    fn bring_puts_the_target_level_in_front_facing_you() {
        let host = fake(0);
        assert_eq!(
            bring(&[], &host).unwrap(),
            "amtele 4 99.999992 300.000000 48.000000 270.000000"
        );
        // With a distance the view's pitch counts and nothing is lifted.
        assert_eq!(
            bring(&args("kyle 50 10"), &host).unwrap(),
            "amtele 4 100.000000 243.301270 -1.000000 280.000000"
        );
    }

    #[test]
    fn goto_uses_the_drawn_player_or_asks_the_server() {
        let (command, note) = go_to(&[], &fake(0)).unwrap();
        assert_eq!(command, "amtele 100.000000 0.000000 40.000000 180.000000");
        assert!(note.is_none());
        // A name alone lifts you above the player's sent position.
        let (command, _) = go_to(&args("gun"), &fake(0)).unwrap();
        assert_eq!(command, "amtele 100.000000 0.000000 56.000000 180.000000");
        let (command, note) = go_to(&[], &fake(1_000)).unwrap();
        assert_eq!(command, "amtele 4");
        assert!(note.is_some());
    }

    #[test]
    fn offset_moves_by_whole_units() {
        assert_eq!(
            offset(&args("10.9 -5"), me()).unwrap(),
            "amtele 110.000000 195.000000 24.000000 90.000000"
        );
        assert!(offset(&[], me()).is_err());
        assert!(offset(&args("1 2 3 4"), me()).is_err());
    }

    #[test]
    fn a_mark_recalls_through_setviewpos() {
        let mark = Mark::of(me());
        assert_eq!(mark.describe(), "Mark set (100 200 24) : 90");
        assert_eq!(mark.recall(), "setviewpos 100 200 24 90");
    }
}
