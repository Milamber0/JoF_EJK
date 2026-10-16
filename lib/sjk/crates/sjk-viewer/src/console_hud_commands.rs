//! jaPRO cg_consolecmds.c:1134-1198,1529-1590: bit frontends for existing HUDs.
use super::super::ViewerConsole;

const SPEED: &[(u32, &str)] = &[
    (0, "Enable speedometer"),
    (8, "Kilometers per hour"),
    (9, "Miles per hour"),
    (15, "XYZ speed"),
];
const STRAFE: &[(u32, &str)] = &[
    (1, "Lines (JoF EJK updated style)"),
    (2, "Airborne CGAZ"),
    (5, "W"),
    (6, "WA"),
    (7, "WD"),
    (8, "A"),
    (9, "D"),
    (10, "Rear (lines)"),
    (15, "S"),
    (16, "SA"),
    (17, "SD"),
];

/// Apply a supported reference bit option to the existing retained HUD's cvar.
pub(super) fn execute(
    console: &mut ViewerConsole,
    command: &str,
    args: &[String],
) -> Result<Vec<String>, String> {
    let (cvar, entries) = if command == "speedometer" {
        ("cg_speedometer", SPEED)
    } else {
        ("cg_strafeHelper", STRAFE)
    };
    let old = console.integer_cvar(cvar).unwrap_or(0);
    if args.is_empty() {
        return Ok(entries
            .iter()
            .map(|(bit, label)| {
                format!(
                    "{bit}: [{}] {label}",
                    if old & (1 << bit) != 0 { "X" } else { " " }
                )
            })
            .collect());
    }
    let [value] = args else {
        return Err(format!("usage: {command} [option number]"));
    };
    let bit: u32 = value.parse().map_err(|_| "Expected an option number")?;
    if !entries.iter().any(|(index, _)| *index == bit) {
        return Err(format!(
            "{command}: option {bit} is not supported by SJK's display"
        ));
    }
    let next = toggled(command, old, bit);
    console.set_cvar(cvar, &next.to_string());
    Ok(vec![format!("{cvar} = {next}")])
}

fn toggled(command: &str, value: i64, bit: u32) -> i64 {
    let group = if command == "speedometer" && matches!(bit, 8 | 9) {
        (1 << 8) | (1 << 9)
    } else if command == "strafehelper" && matches!(bit, 1 | 2) {
        15 | (1 << 13)
    } else {
        0
    };
    let mask = if command == "speedometer" {
        0xffff
    } else {
        0x1fffff
    };
    let value = if group == 0 {
        value & mask
    } else {
        value & !(group & !(1 << bit))
    };
    value ^ (1 << bit)
}
