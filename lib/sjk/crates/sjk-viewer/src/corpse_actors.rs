//! Reliable body copies have independent geometry, even after the owner changes.

use super::*;
use sjk_client::BaseServerCommandEvent;

impl GpuState {
    /// Process copies once per death, never by allocating in the entity loop.
    pub(crate) fn apply_body_commands(&mut self, presentation_time: i64) {
        if self.clientinfo_watch.body_commands.is_empty() {
            return;
        }
        let mut commands = std::mem::take(&mut self.clientinfo_watch.body_commands);
        for command in commands.drain(..) {
            match command {
                BaseServerCommandEvent::CopyBody(body) => {
                    if let Err(error) = self.copy_body_actor(body, presentation_time) {
                        crate::log::progress(format_args!("body copy failed: {error}"));
                    }
                }
                BaseServerCommandEvent::KillGhoul2(number) => {
                    let id = EntityId::new(u64::from(number) + 1);
                    for mesh in &mut self.actor_meshes {
                        if mesh.corpse_pool && mesh.entity_id == Some(id) {
                            mesh.entity_id = None;
                            mesh.body_identity = None;
                            mesh.body_copied = false;
                        }
                    }
                }
                _ => {}
            }
        }
        self.clientinfo_watch.body_commands = commands;
    }

    fn copy_body_actor(
        &mut self,
        mut body: sjk_client::BodyIdentity,
        time: i64,
    ) -> Result<(), Box<dyn Error>> {
        // The body of a player muted on this PC looks as they did: Kyle, the default saber.
        if self.muted_players.contains(u16::from(body.client_num)) {
            body.appearance = crate::muted_players::appearance(&body.appearance);
            crate::muted_players::sabers(&mut body.sabers);
        }
        let id = EntityId::new(u64::from(body.entity_num) + 1);
        for name in body.sabers.iter().flatten() {
            self.load_hilt(name)?;
        }
        // Reuse only storage with the same appearance. Each active body needs
        // its own vertex ranges; sharing the live mesh overwrites its pose.
        let reusable = self.actor_meshes.iter().position(|mesh| {
            mesh.corpse_pool
                && mesh.appearance == body.appearance
                && (mesh.entity_id.is_none() || mesh.entity_id == Some(id))
        });
        if let Some(index) = reusable {
            // The slot's previous body, of another model, must not keep drawing here:
            // the first mesh found for an entity is the one submitted.
            self.release_body_slot(id);
            let mesh = &mut self.actor_meshes[index];
            mesh.entity_id = Some(id);
            mesh.saber_names = body.sabers.clone();
            let client_num = body.client_num;
            mesh.body_identity = Some(body);
            mesh.disintegration = None;
            let animation = mesh.preview.animation.clone();
            let config = mesh.preview.config.clone();
            let copied = self.body_queue_animator(client_num, &animation, &config, time)?;
            let mesh = &mut self.actor_meshes[index];
            mesh.body_copied = copied.is_some();
            mesh.animator = match copied {
                Some(animator) => animator,
                None => crate::actor_pose::storage(&mesh.preview.animation)?,
            };
            self.copy_body_surfaces(client_num, index);
            return Ok(());
        }
        let preview = self
            .actor_meshes
            .iter()
            .find(|mesh| !mesh.corpse_pool && mesh.appearance == body.appearance)
            .map(|mesh| mesh.preview.clone());
        let mut mesh = if let Some(preview) = preview {
            self.upload_actor(preview, &body.appearance, id, body.sabers.clone())?
        } else {
            self.build_live_actor_or_kyle(&body.appearance, id, body.sabers.clone())?
        };
        mesh.corpse_pool = true;
        if let Some(animator) = self.body_queue_animator(
            body.client_num,
            &mesh.preview.animation,
            &mesh.preview.config,
            time,
        )? {
            mesh.animator = animator;
            mesh.body_copied = true;
        }
        mesh.body_identity = Some(body);
        let client_num = mesh.body_identity.as_ref().map(|body| body.client_num);
        self.release_body_slot(id);
        self.actor_meshes.push(mesh);
        self.actor_groups.push(Vec::with_capacity(4));
        if let Some(client_num) = client_num {
            self.copy_body_surfaces(client_num, self.actor_meshes.len() - 1);
        }
        Ok(())
    }

    /// `CG_BodyQueueCopy` duplicates the instance, so the body keeps the limbs its
    /// player lost (`dismember`); other bodies start whole.
    fn copy_body_surfaces(&mut self, client_num: u8, body: usize) {
        let source_id = EntityId::new(u64::from(client_num) + 1);
        let source = self
            .actor_meshes
            .iter()
            .find(|mesh| {
                !mesh.corpse_pool && mesh.limb.is_none() && mesh.entity_id == Some(source_id)
            })
            .filter(|mesh| mesh.appearance == self.actor_meshes[body].appearance)
            .map(|mesh| mesh.surfaces.clone());
        let mesh = &mut self.actor_meshes[body];
        mesh.surfaces.reset(&mesh.preview.mesh.hierarchy);
        if let Some(source) = source {
            mesh.surfaces
                .copy_from(&source, &mesh.preview.mesh.hierarchy);
        }
    }

    /// Return the bodies drawn for entity `id` to the pool under their own appearance.
    ///
    /// Replacing one instead threw its uploaded geometry away, so the next death of
    /// that model uploaded it again: on a map where a crowd dies every second (one
    /// spawn point, telefrags) that was a full actor upload several times a second,
    /// into shared buffers that only grow.
    fn release_body_slot(&mut self, id: EntityId) {
        for old in &mut self.actor_meshes {
            if old.corpse_pool && old.entity_id == Some(id) {
                old.entity_id = None;
                old.body_identity = None;
                old.body_copied = false;
            }
        }
    }

    /// `CG_BodyQueueCopy`'s Ghoul2 copy: client `client_num`'s animator as last
    /// presented, duplicated with the body animation installed at `time`. `None` when
    /// the client was never presented here or its skeleton differs from the body's;
    /// the body then holds its completed pose.
    fn body_queue_animator(
        &self,
        client_num: u8,
        animation: &sjk_model::Gla,
        config: &sjk_model::AnimationConfig,
        time: i64,
    ) -> Result<Option<crate::actor_pose::evaluation::Slot>, Box<dyn Error>> {
        let source_id = EntityId::new(u64::from(client_num) + 1);
        let Some(source) = self
            .actor_meshes
            .iter()
            .find(|mesh| !mesh.corpse_pool && mesh.entity_id == Some(source_id))
        else {
            return Ok(None);
        };
        if source.preview.animation.bones.len() != animation.bones.len() {
            return Ok(None);
        }
        let Some((clip, frame)) = source.animator.presented_torso(&source.preview.animation) else {
            return Ok(None);
        };
        let Some(command) = sjk_client::legacy_body_queue_command(config, clip, frame, time) else {
            return Ok(None);
        };
        let animator = source.animator.body_queue_copy(animation, command)?;
        Ok(Some(crate::actor_pose::evaluation::Slot::from_animator(
            animator,
        )))
    }
}
