//! One retained leader/opponent portrait in the existing HUD icon atlas.
use sjk_protocol::GameState;
use sjk_ui::{Color, DrawCommand, DrawList, Rect, TextureId};

// The HUD's bg_itemlist slot zero has no item/image; menu cells precede it.
const TEXTURE: TextureId = TextureId(crate::ui_renderer::ICON_CELLS);

#[derive(Default)]
/// Cached selected-client art; an unresolved icon deliberately emits nothing.
pub(super) struct Portrait {
    model: Vec<u8>,
    ready: bool,
}

fn model(game: &GameState, client: Option<u16>) -> &[u8] {
    client
        .and_then(|id| game.config_string(1131 + usize::from(id)))
        .and_then(|bytes| sjk_client::LegacyClientInfo::new(bytes).bytes("model"))
        .unwrap_or_default()
}

/// The head icon of a clientinfo `model` value (`models/players/<model>/icon_<skin>`).
pub(crate) fn icon_path(model: &[u8]) -> Option<String> {
    if model.is_empty() {
        return None;
    }
    // Only the asset field is interpreted, never the enclosing (byte-string) clientinfo.
    let value = String::from_utf8_lossy(model);
    let (model, skin) = value.split_once('/').unwrap_or((&value, "default"));
    // Customizable species identify their head icon before torso/legs selections.
    let skin = skin
        .split('|')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("default");
    Some(format!("models/players/{model}/icon_{skin}"))
}

impl Portrait {
    /// Resolve only when selected model bytes change, retaining misses until then as well.
    pub(super) fn update(
        &mut self,
        game: &GameState,
        client: Option<u16>,
        vfs: &sjk_vfs::VirtualFileSystem,
        shaders: &sjk_shader::ShaderCatalog,
        renderer: &crate::ui_renderer::ShapeRenderer,
        queue: &crate::frame_queue::FrameQueue,
    ) {
        self.set_model(model(game, client), |path| {
            let Some(pixels) = super::icons::assets::decode(vfs, shaders, path) else {
                crate::log::progress(format_args!("HUD portrait absent, not drawn: {path}"));
                return false;
            };
            renderer.upload_icon(queue, TEXTURE, pixels.as_raw());
            true
        });
    }

    fn set_model(&mut self, value: &[u8], resolve: impl FnOnce(&str) -> bool) {
        if self.model == value {
            return;
        }
        self.model.clear();
        self.model.extend_from_slice(value);
        self.ready = false;
        let Some(path) = icon_path(value) else {
            return;
        };
        self.ready = resolve(&path);
    }

    /// Height this portrait occupies above the leader/opponent text, zero when unresolved.
    pub(super) const HEIGHT: f32 = 92.0;

    /// Whether an image resolved, and so whether [`Portrait::HEIGHT`] applies to the layout.
    pub(super) fn drawn(&self) -> bool {
        self.ready
    }

    /// Emit the resolved portrait above the leader/opponent text, sharing its right edge.
    pub(super) fn emit(&self, list: &mut DrawList, viewport: [f32; 2], top: f32) {
        if !self.ready {
            return;
        }
        let s = crate::ui_scale::height_scale(viewport[1]);
        let _ = list.push(DrawCommand::TexturedQuad {
            rect: Rect::new(viewport[0] - 128.0 * s, top * s, 88.0 * s, 88.0 * s),
            texture: TEXTURE,
            color: Color::new(1.0, 1.0, 1.0, 1.0),
        });
    }
}
