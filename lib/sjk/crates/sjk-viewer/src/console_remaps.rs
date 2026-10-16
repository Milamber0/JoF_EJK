//! Tayst-compatible local shader commands, owned by the displayed map.
impl crate::GpuState {
    pub(super) fn remap_command(
        &mut self,
        name: &str,
        args: &[String],
    ) -> Result<Vec<String>, String> {
        if name == "remapshader" {
            let [old, new] = args else {
                return Err("usage: remapShader <old> <new>".into());
            };
            self.world_materials.local_remap(
                self.vfs.as_deref().ok_or("No loaded map")?,
                &self.shaders,
                old,
                new,
            )?;
            return Ok(vec![format!("{old} -> {new}")]);
        }
        if name == "clearremaps" {
            if !args.is_empty() {
                return Err("usage: clearRemaps".into());
            }
            // EternalJK's renderer command: local only, nothing reaches the server.
            if let Some(session) = &mut self.live_session {
                session.clear_shader_remaps();
            } else if let Some(session) = &mut self.demo_session {
                session.clear_shader_remaps();
            } else {
                self.config_string_refresh.clear_remaps();
            }
            self.world_materials.clear_local_remaps();
            return Ok(vec!["Shader remaps cleared".into()]);
        }
        if !args.is_empty() {
            return Err("usage: listRemaps".into());
        }
        let mode = self.remap_mode();
        let state = self
            .live_session
            .as_ref()
            .map(|s| s.shader_remaps())
            .or_else(|| self.demo_session.as_ref().map(|s| s.shader_remaps()));
        let mut lines = self
            .world_materials
            .remap_listing(state.and_then(|s| s.table(mode)));
        if lines.is_empty() {
            lines.push("No active shader remaps".into());
        }
        if mode == 0 && self.console.as_ref().is_some_and(|c| c.remap_mode() != 0) {
            lines.insert(
                0,
                format!(
                    "Server shader remaps ignored on {} ({})",
                    crate::remap_blocked_maps::map_name(&self.world_load_map),
                    crate::remap_blocked_maps::CVAR,
                ),
            );
        }
        Ok(lines)
    }
}
