//! Completion/help metadata for locally dispatched selection commands.

/// Register metadata; execution is intercepted by GameplayInput before shell fallback.
pub(super) fn register(shell: &mut sjk_shell::Shell) -> Result<(), sjk_shell::CommandError> {
    for (name, help) in [
        (
            "forcenext",
            "Select next usable Force power (Use held: inventory)",
        ),
        (
            "forceprev",
            "Select previous usable Force power (Use held: inventory)",
        ),
        (
            "forceselect",
            "Select Force wheel entry N (forcePowers_t number; 18-20 JoF's)",
        ),
        (
            "weapmelee",
            "Select the melee weapon (fists), never the saber; does nothing if already selected",
        ),
        ("invnext", "Select next inventory item"),
        ("invprev", "Select previous inventory item"),
        (
            "toy_illuminate",
            "Toy: turn the Illuminate holocron's light on or off",
        ),
    ] {
        shell.commands.register(name, help, |_| {
            Err(sjk_shell::CommandError::Handler(
                "Viewer input dispatcher required".into(),
            ))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The toy is listed, with its help; the old name runs it (the input dispatcher
    /// takes it, `input.rs`) but is registered nowhere, so completion and `cmdlist`
    /// never show it.
    #[test]
    fn the_toy_is_listed_and_its_old_name_is_not() {
        let mut shell = sjk_shell::Shell::new(
            sjk_shell::CvarRegistry::default(),
            sjk_shell::BindTable::default(),
        );
        register(&mut shell).unwrap();
        assert!(shell.commands.contains("toy_illuminate"));
        assert!(!shell.commands.contains("force_illuminate"));
        let help = shell
            .commands
            .iter()
            .find(|(name, _)| *name == "toy_illuminate")
            .map(|(_, help)| help)
            .unwrap();
        assert!(!help.contains("cg_illuminate") && !help.contains("Force wheel"));
        let forceselect = shell
            .commands
            .iter()
            .find(|(name, _)| *name == "forceselect")
            .map(|(_, help)| help)
            .unwrap();
        assert!(!forceselect.contains("Illuminate"));
    }
}
