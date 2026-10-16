//! Stock input cvars: codemp/client/cl_main.cpp:2766-2811.
use super::motion::Motion;

/// Expose actual viewer input handlers to completion, help, and cmdlist.
/// Dispatch is owned by the viewer; invoking the registry without it fails explicitly.
pub(crate) fn register_commands(
    shell: &mut sjk_shell::Shell,
) -> Result<(), sjk_shell::CommandError> {
    let buttons = [
        "forward",
        "back",
        "moveleft",
        "moveright",
        "moveup",
        "movedown",
        "duck",
        "left",
        "right",
        "lookup",
        "lookdown",
        "strafe",
        "mlook",
        "speed",
        "attack",
        "altattack",
        "use",
        "useforce",
        "force",
        "force_grip",
        "force_lightning",
        "force_drain",
        "grapple",
        "scores",
    ];
    for name in buttons
        .iter()
        .map(|name| name.to_string())
        .chain((0..16).map(|index| format!("button{index}")))
    {
        for prefix in ['+', '-'] {
            shell
                .commands
                .register(&format!("{prefix}{name}"), "Held input action", |_| {
                    Err(sjk_shell::CommandError::Handler(
                        "Viewer input dispatcher required".into(),
                    ))
                })?;
        }
    }
    for (name, _) in super::generic_commands::GENERIC_COMMANDS {
        shell
            .commands
            .register(name, "Stock generic input command", |_| {
                Err(sjk_shell::CommandError::Handler(
                    "Viewer input dispatcher required".into(),
                ))
            })?;
    }
    shell.commands.register(
        "centerview",
        "Center pitch using snapshot delta angles",
        |_| {
            Err(sjk_shell::CommandError::Handler(
                "Viewer input dispatcher required".into(),
            ))
        },
    )?;
    super::selection_commands::register(shell)?;
    Ok(())
}

use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry};

/// Register only settings consumed by the input path.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), CvarError> {
    for (name, value, help) in [
        ("cl_yawspeed", 140.0, "Keyboard yaw degrees per second"),
        ("cl_pitchspeed", 140.0, "Keyboard pitch degrees per second"),
        (
            "cl_anglespeedkey",
            1.5,
            "Turning multiplier while speed is held",
        ),
        ("m_forward", 0.25, "Mouse forward movement scale"),
        ("m_side", 0.25, "Mouse sideways movement scale"),
        (
            "cl_mouseAccel",
            0.0,
            "Mouse acceleration strength or exponent",
        ),
        ("cl_mouseAccelOffset", 5.0, "Style 1 acceleration offset"),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    cvars.register(CvarDefinition::new(
        "cl_mouseAccelStyle",
        0_i64,
        CvarFlags::ARCHIVE,
        "0 legacy; 1 power acceleration",
    ))?;
    for (name, value, help) in [
        ("cl_freelook", true, "Mouse y controls pitch"),
        (
            "m_filter",
            false,
            "Average mouse counts with the preceding frame",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    Ok(())
}

impl Motion {
    pub(crate) fn sync(&mut self, console: &crate::console::ViewerConsole) {
        self.yaw_speed = console.float_cvar("cl_yawspeed").unwrap_or(140.0) as f32;
        self.pitch_speed = console.float_cvar("cl_pitchspeed").unwrap_or(140.0) as f32;
        self.angle_speed_key = console.float_cvar("cl_anglespeedkey").unwrap_or(1.5) as f32;
        self.m_forward = console.float_cvar("m_forward").unwrap_or(0.25) as f32;
        self.m_side = console.float_cvar("m_side").unwrap_or(0.25) as f32;
        self.accel = console.float_cvar("cl_mouseaccel").unwrap_or(0.0) as f32;
        self.accel_style = console.integer_cvar("cl_mouseaccelstyle").unwrap_or(0);
        self.accel_offset = console
            .float_cvar("cl_mouseacceloffset")
            .unwrap_or(5.0)
            .max(0.001) as f32;
        self.freelook = console.bool_cvar("cl_freelook").unwrap_or(true);
        self.filter = console.bool_cvar("m_filter").unwrap_or(false);
    }
}
