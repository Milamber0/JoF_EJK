//! Player-facing cgame presentation options; legacy names stay in the viewer adapter.
use crate::console::ViewerConsole;
use sjk_shell::{CvarDefinition, CvarFlags, CvarRegistry};

/// Register only options consumed by presentation paths in this batch.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    crate::hud::family::register(cvars)?;
    crate::hud::kill_feed::register(cvars)?;
    crate::hud::movement::register(cvars)?;
    for (name, value, help) in [
        (
            "cg_chatBox",
            10000,
            "Chat lifetime in milliseconds; zero hides the feed",
        ),
        (
            "cg_chatBoxLines",
            5,
            "Visible conversation entries, bounded to eight",
        ),
        (
            "cg_chatBoxShowHistory",
            0,
            "Show expired chat while the console is open",
        ),
        (
            "cg_cleanChatbox",
            0,
            "Clean body colours and suppress repeated messages",
        ),
        ("cg_chatSounds", 1, "Master chat notification switch"),
        (
            crate::chat::reply::REMEMBER_CVAR,
            1,
            "The chat key opens on the channel of the last message sent (All, Team, SJK or a whisper)",
        ),
        (
            crate::chat::emoji::CVAR,
            0,
            "Show emoji pictures (gfx/emoji) in place of their names in chat; listEmojis lists them",
        ),
        (
            "cg_fovViewmodelAdjust",
            1,
            "Lower the first-person weapon when cg_fovViewmodel is over 90",
        ),
        (
            "cg_crossHairScope",
            0,
            "Scope: 0 stock, 1 crosshair, 2 mask only",
        ),
        ("cg_autoSwitch", 1, "Pickup upgrades: 0 off, 1 safe, 2 any"),
        (
            "cg_screenShake",
            2,
            "Camera shakes from effect files (explosions): 0 off, else on. EternalJK's level 2 charge and recoil shakes, and 0 switching off the damage kick, are not implemented. Your own muzzle flash never shakes",
        ),
    ] {
        cvars.register(CvarDefinition::new(
            name,
            value as i64,
            CvarFlags::ARCHIVE,
            help,
        ))?;
    }
    for (name, value, help) in [
        (
            "cg_chatBoxHeight",
            350.0,
            "Chat bottom in virtual 480-high coordinates",
        ),
        ("cg_chatBoxFontSize", 1.0, "Conversation font scale"),
        (
            "cg_chatBoxLetterSpacing",
            -0.5,
            "Extra space between chat message letters, in 1080p pixels (-2 to 8)",
        ),
        (
            "cg_fovViewmodel",
            80.0,
            "First-person weapon field of view (EternalJK default 80); zero is the retail look, drawn with cg_fov",
        ),
        (
            "cg_chatBoxX",
            30.0,
            "Chat left in virtual 640-wide coordinates",
        ),
        (
            "cg_chatBoxCutOffLength",
            350.0,
            "Chat wrapping width in virtual units",
        ),
        ("cg_bobUp", 0.005, "Vertical view bob scalar"),
        ("cg_bobPitch", 0.002, "View pitch bob scalar"),
        ("cg_bobRoll", 0.002, "View roll bob scalar"),
        ("cg_centerTime", 3.0, "Centre-print lifetime in seconds"),
        ("cg_centerSize", 1.0, "Centre-print font scale"),
        (
            "cg_centerHeight",
            0.0,
            "Centre-print virtual Y; zero uses hero placement",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    cvars.register(CvarDefinition::new(
        "cg_crosshairColor",
        "0 0 0 255",
        CvarFlags::ARCHIVE,
        "RGBA bytes; black uses default",
    ))?;
    Ok(())
}

/// Scope style: stock art (0), ordinary crosshair (1), or mask without rotating insert (2).
pub(crate) fn scope_style(console: Option<&ViewerConsole>) -> i64 {
    console
        .and_then(|c| c.integer_cvar("cg_crosshairscope"))
        .unwrap_or(0)
}

/// Mask and rotating-insert visibility used directly by the GPU quad builder.
pub(crate) fn scope_layers(style: i64) -> [bool; 2] {
    [style != 1, style == 0]
}

/// Master chat-sound switch, composed with the existing independent beep controls.
pub(crate) fn chat_sounds(console: Option<&ViewerConsole>) -> bool {
    console
        .and_then(|c| c.integer_cvar("cg_chatsounds"))
        .unwrap_or(1)
        != 0
}

/// Finite scalar lookup using canonical lowercase names, without temporary strings.
pub(crate) fn scalar(console: Option<&ViewerConsole>, name: &str, default: f64) -> f32 {
    console
        .and_then(|c| c.float_cvar(name))
        .filter(|v| v.is_finite())
        .unwrap_or(default) as f32
}

/// EternalJK `cg_screenShake`: nonzero lets effect files shake the camera
/// (`CG_FX_CameraShake`, `cg_main.c:3828-3832`). Only that is implemented: the
/// level 2 shakes of a charging or just-fired weapon (`cg_weapons.c:793,801,2466`) and the
/// switch that turns the damage view kick off (`cg_view.c:1145`) are not.
pub(crate) fn screen_shake(console: Option<&ViewerConsole>) -> bool {
    console
        .and_then(|c| c.integer_cvar("cg_screenshake"))
        .unwrap_or(2)
        != 0
}

/// Stock bob scalars affect only the rendered camera, never usercmd angles.
pub(crate) fn bob(console: Option<&ViewerConsole>) -> sjk_client::LegacyViewBobConfig {
    sjk_client::LegacyViewBobConfig {
        bob_up: scalar(console, "cg_bobup", 0.005),
        bob_pitch: scalar(console, "cg_bobpitch", 0.002),
        bob_roll: scalar(console, "cg_bobroll", 0.002),
        ..Default::default()
    }
}

/// TaystJK uses an all-black RGB value as the default crosshair-colour sentinel,
/// which draws the picture untinted (`R_SetColor(NULL)`).
pub(crate) fn crosshair_color(console: Option<&ViewerConsole>) -> [f32; 4] {
    let mut rgba = [0.0, 0.0, 0.0, 255.0];
    let text = console
        .and_then(|c| c.text_cvar("cg_crosshaircolor").ok())
        .unwrap_or("");
    let mut words = text.split_whitespace();
    for slot in &mut rgba {
        let Some(value) = words
            .next()
            .and_then(|word| word.parse::<f32>().ok())
            .filter(|v| v.is_finite())
        else {
            return [1.0, 1.0, 1.0, 1.0];
        };
        *slot = if value < 1.0 { 0.0 } else { value.min(255.0) };
    }
    if rgba[..3] == [0.0; 3] {
        return [1.0, 1.0, 1.0, rgba[3] / 255.0];
    }
    rgba.map(|value| value / 255.0)
}

impl crate::GpuState {
    /// Synchronize retained chat policy and append the existing overlay.
    pub(crate) fn append_configured_chat(
        &mut self,
        viewport: [f32; 2],
        scale: f32,
        scoreboard: bool,
    ) {
        self.chat.configure(self.console.as_ref());
        self.chat
            .set_scoreboard_layout(crate::scoreboard::behavior::chat_layout(self, scoreboard));
        let visible = self
            .console
            .as_ref()
            .and_then(|c| c.bool_cvar("cg_drawchat"))
            .unwrap_or(true);
        self.chat.append(
            visible,
            &mut self.game_fonts,
            &mut self.text_vertices,
            &self.ui_font,
            viewport,
            scale,
        );
    }
}
