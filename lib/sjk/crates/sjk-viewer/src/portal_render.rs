//! Keeping the destination world in step with the join.
use super::*;

impl GpuState {
    /// Keep the preview in step with the join: start it when the target map
    /// becomes known and drop it when the shell leaves the connecting states.
    pub(crate) fn drive_portal(&mut self) {
        let map = self.destination_map();
        if let (Some(map), Some(session)) = (map.as_deref(), self.resident.session.as_ref()) {
            self.portal.aim_session(
                map,
                &self.game_data,
                session.game_state(),
                Some(session.latest_snapshot()),
            );
        } else {
            self.portal.aim(map.as_deref(), self.vfs.as_ref());
        }
        let size = [self.size.width, self.size.height];
        self.portal.poll(&self.context, size, &self.game_data);
    }

    /// `maps/<map>.bsp` the client is joining, if it is joining and the map
    /// is known: the server's gamestate first, else the browser row.
    fn destination_map(&self) -> Option<String> {
        let menu = self.client_menu.as_ref()?;
        // Only a join from the menu world prepares its destination here; a map
        // change on a server reloads through the session transition.
        if !self.is_menu_world || !menu.is_connecting() {
            return None;
        }
        if self.resident.session.is_some() {
            return self.pending_map_path().ok();
        }
        if !self.world_load_map.is_empty() {
            return Some(self.world_load_map.clone());
        }
        menu.destination_map().map(|map| format!("maps/{map}.bsp"))
    }
}
