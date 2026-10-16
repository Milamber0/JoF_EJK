//! What each player wears and where it is drawn. A player's pieces are
//! resolved when their actor mesh is built or their clientinfo changes
//! ([`Worn::invalidate`]): the names in `c1`/`c2`, the models (loaded into
//! the rigid-model set like a mid-match configstring model) and the fitting
//! offsets for the model worn. Each evaluated pose then refreshes the bolts
//! of the worn slots only, and [`submit`] places the pieces with the body,
//! following `CG_DrawCosmeticOnPlayer`'s rules.

use super::{Visibility, fitting_offset, model_path, placement};
use crate::actor_instance::ActorInstance;
use crate::bolt::BoltMatrix;
use sjk_client::CosmeticSlot;
use sjk_runtime::Appearance;

/// `CS_PLAYERS` (`bg_public.h`).
const CS_PLAYERS: usize = 1_131;
/// Bolt each slot is worn on.
const BOLTS: [&str; 2] = ["*head_top", "*back"];

/// One worn piece: its rigid model and fitting offset.
#[derive(Clone, Copy, Debug, PartialEq)]
struct WornPiece {
    mesh: usize,
    offset: [f32; 3],
}

/// What one actor wears, and the bolts of its latest pose.
#[derive(Clone, Debug, Default)]
pub(crate) struct Worn {
    /// Clientinfo has been read since the mesh was built or last changed.
    resolved: bool,
    pieces: [Option<WornPiece>; 2],
    /// Raw `*head_top` and `*back` bolts of the latest pose, worn slots only.
    bolts: [Option<BoltMatrix>; 2],
}

impl Worn {
    /// Read the clientinfo again before the next frame.
    pub(crate) fn invalidate(&mut self) {
        self.resolved = false;
    }

    /// Refresh the bolts of the worn slots from one evaluated pose; nothing
    /// is computed for an actor wearing nothing.
    pub(crate) fn update_bolts(
        &mut self,
        preview: &crate::PlayerPreview,
        matrices: &[[[f32; 4]; 3]],
    ) {
        for (index, bolt) in self.bolts.iter_mut().enumerate() {
            *bolt = self.pieces[index].and_then(|_| preview.bolt(BOLTS[index], matrices));
        }
    }
}

impl crate::GpuState {
    /// Resolve the actors whose pieces are not known yet: just built (a map
    /// load) or whose clientinfo changed. Idle frames only test a flag each.
    pub(crate) fn refresh_cosmetics(&mut self) {
        if self.actor_meshes.iter().all(|mesh| mesh.cosmetics.resolved) {
            return;
        }
        for index in 0..self.actor_meshes.len() {
            if !self.actor_meshes[index].cosmetics.resolved {
                self.resolve_cosmetics(index);
            }
        }
    }

    fn resolve_cosmetics(&mut self, index: usize) {
        let worn = &mut self.actor_meshes[index].cosmetics;
        worn.resolved = true;
        worn.pieces = [None; 2];
        worn.bolts = [None; 2];
        let mesh = &self.actor_meshes[index];
        // Corpses and NPCs wear nothing (`CG_DrawCosmeticOnPlayer` skips the dead).
        let Some(client) = mesh
            .entity_id
            .filter(|_| !mesh.corpse_pool)
            .and_then(|id| usize::try_from(id.get().checked_sub(1)?).ok())
            .filter(|client| *client < 32)
        else {
            return;
        };
        let game_state = self
            .live_session
            .as_ref()
            .map(crate::ClientSession::game_state)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(crate::demo_playback::Session::game_state)
            });
        let Some(info) = game_state.and_then(|game| game.config_string(CS_PLAYERS + client)) else {
            return;
        };
        let names =
            CosmeticSlot::ALL.map(|slot| sjk_client::worn_cosmetic(info, slot).map(str::to_owned));
        if names.iter().all(Option::is_none) {
            return;
        }
        let Some(vfs) = self.vfs.clone() else {
            return;
        };
        let (model, skin) = (
            mesh.appearance.model.clone(),
            mesh.appearance.variant.clone(),
        );
        let mut pieces = [None; 2];
        for (slot, name) in CosmeticSlot::ALL.into_iter().zip(&names) {
            let Some(name) = name else {
                continue;
            };
            // A piece this client does not have is simply not drawn.
            let Some(path) = model_path(&vfs, slot, name) else {
                continue;
            };
            let Some(mesh) = self.cosmetic_mesh(&path) else {
                continue;
            };
            let offset = fitting_offset(&vfs, slot, name, &model, &skin);
            pieces[slot.index()] = Some(WornPiece { mesh, offset });
        }
        self.actor_meshes[index].cosmetics.pieces = pieces;
    }

    /// The rigid model at `path`, loaded on first use.
    fn cosmetic_mesh(&mut self, path: &str) -> Option<usize> {
        let find = |meshes: &[crate::StaticModelMesh]| {
            meshes.iter().position(|mesh| {
                mesh.appearance.variant.is_empty() && mesh.appearance.model == path
            })
        };
        if let Some(index) = find(&self.object_meshes) {
            return Some(index);
        }
        let appearance = Appearance {
            model: path.to_owned(),
            variant: String::new(),
        };
        if let Err(error) = self.load_config_model(&appearance) {
            crate::log::progress(format_args!("cosmetic {path}: load failed: {error}"));
            return None;
        }
        find(&self.object_meshes)
    }
}

/// Everything [`submit`] needs to know about the actor's frame.
pub(crate) struct Frame {
    pub(crate) transform: sjk_runtime::Transform,
    /// `cg_cosmetics`.
    pub(crate) visibility: Visibility,
    /// The actor is the local player.
    pub(crate) local: bool,
    /// The body is drawn in the main view (not the first-person local player).
    pub(crate) draw_actor: bool,
    pub(crate) view_flags: u32,
    /// The body is fading in or out of a mind trick (`CG_IsMindTricked`).
    pub(crate) tricked: bool,
    /// `EF_DEAD` on the entity.
    pub(crate) dead: bool,
}

/// Bolt the actor's pieces on for this frame, under `CG_DrawCosmeticOnPlayer`'s
/// rules: none on the dead, the tricked or a scaled model, the local
/// first-person player's only in mirrors and portals.
pub(crate) fn submit(worn: &Worn, frame: &Frame, object_groups: &mut [Vec<ActorInstance>]) {
    if frame.dead
        || frame.tricked
        || !frame.visibility.shows(frame.local)
        || frame.transform.scale != [1.0; 3]
    {
        return;
    }
    for (piece, bolt) in worn.pieces.iter().zip(worn.bolts) {
        let (Some(piece), Some(bolt)) = (piece, bolt) else {
            continue;
        };
        let Some(mut instance) = placement(bolt, frame.transform, piece.offset) else {
            continue;
        };
        instance.view_flags = frame.view_flags | u32::from(!frame.draw_actor);
        if let Some(group) = object_groups.get_mut(piece.mesh) {
            group.push(instance);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Frame {
        Frame {
            transform: sjk_runtime::Transform::IDENTITY,
            visibility: Visibility::On,
            local: false,
            draw_actor: true,
            view_flags: 0,
            tricked: false,
            dead: false,
        }
    }

    fn worn() -> Worn {
        let bolt = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 60.0],
        ];
        Worn {
            resolved: true,
            pieces: [
                Some(WornPiece {
                    mesh: 1,
                    offset: [0.0; 3],
                }),
                None,
            ],
            bolts: [Some(bolt), Some(bolt)],
        }
    }

    #[test]
    fn pieces_follow_the_rules_of_cg_draw_cosmetic_on_player() {
        let mut groups = vec![Vec::new(), Vec::new()];
        submit(&worn(), &frame(), &mut groups);
        assert_eq!(groups[1].len(), 1);
        assert!(groups[0].is_empty());
        for hidden in [
            Frame {
                dead: true,
                ..frame()
            },
            Frame {
                tricked: true,
                ..frame()
            },
            Frame {
                visibility: Visibility::OnlyMe,
                ..frame()
            },
            Frame {
                transform: sjk_runtime::Transform {
                    scale: [1.2; 3],
                    ..sjk_runtime::Transform::IDENTITY
                },
                ..frame()
            },
        ] {
            let mut groups = vec![Vec::new(), Vec::new()];
            submit(&worn(), &hidden, &mut groups);
            assert!(groups.iter().all(Vec::is_empty));
        }
        // The first-person local player's hat shows only in mirrors.
        let mut groups = vec![Vec::new(), Vec::new()];
        let first_person = Frame {
            local: true,
            draw_actor: false,
            ..frame()
        };
        submit(&worn(), &first_person, &mut groups);
        assert_eq!(groups[1][0].view_flags, 1);
    }
}
