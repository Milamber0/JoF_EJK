//! Fixed-size texture atlas used by generic retained UI textured quads: a grid
//! of [`ICON_SIZE`] cells for icons. Map previews have their own texture
//! ([`super::levelshot`]).

use super::ShapeVertex;
use sjk_ui::{Color, Rect, TextureId};

const ATLAS_SIZE: u32 = 2_048;
/// Edge of one atlas cell; icons are uploaded at exactly this size.
pub(crate) const ICON_SIZE: u32 = 128;
const COLUMNS: u32 = ATLAS_SIZE / ICON_SIZE;
/// Original menu-cell reservation (13 rows); HUD cells follow without reducing
/// menu capacity.
pub(crate) const ICON_CELLS: u32 = COLUMNS * 13;
/// HUD icon cells, right after the menu cells (`hud::icons::assets::COUNT`).
const HUD_CELLS: u32 = 120;
/// First of the player screen's Force page cells, after the HUD cells, so the
/// Force art never takes cells from the character grid.
pub(crate) const FORCE_ICON_FIRST: u32 = ICON_CELLS + HUD_CELLS;
/// Force page cells: two atlas rows.
pub(crate) const FORCE_ICON_CELLS: u32 = 2 * COLUMNS;
/// First of the 32 classic-scoreboard head-icon cells (one per client slot),
/// after the Force page cells.
pub(crate) const SCOREBOARD_ICON_CELLS: u32 = FORCE_ICON_FIRST + FORCE_ICON_CELLS;
/// First of the character creation cells (one species' part icons and tint
/// base), after the scoreboard's.
pub(crate) const PART_ICON_FIRST: u32 = SCOREBOARD_ICON_CELLS + 32;
/// Character creation cells: four atlas rows.
pub(crate) const PART_ICON_CELLS: u32 = 4 * COLUMNS;
/// First of the key-binding panel's cells (weapon, item and Force icons),
/// after character creation's.
pub(crate) const BIND_ICON_FIRST: u32 = PART_ICON_FIRST + PART_ICON_CELLS;
/// Key-binding panel cells: three atlas rows.
pub(crate) const BIND_ICON_CELLS: u32 = 3 * COLUMNS;
/// First of the HUD Force wheel's cells (the selection bar's power icons and
/// JoF's), after the key-binding panel's.
pub(crate) const FORCE_WHEEL_ICON_FIRST: u32 = BIND_ICON_FIRST + BIND_ICON_CELLS;
/// Force wheel cells: two atlas rows.
pub(crate) const FORCE_WHEEL_ICON_CELLS: u32 = 2 * COLUMNS;
/// The cell holding SJK's emblem (`ui_renderer::LOGO_TEXTURE`), after the Force
/// wheel's: the scoreboard and the player card mark SJK players with it.
pub(crate) const LOGO_ICON: u32 = FORCE_WHEEL_ICON_FIRST + FORCE_WHEEL_ICON_CELLS;
/// First of the classic crosshair pictures' cells (`gfx/2d/crosshaira`..`j`),
/// after the emblem's.
pub(crate) const CROSSHAIR_ICON_FIRST: u32 = LOGO_ICON + 1;
/// Crosshair cells: one per retail picture (`NUM_CROSSHAIRS`).
pub(crate) const CROSSHAIR_ICON_CELLS: u32 = 10;
/// The cell holding the verified badge (`ui_renderer::VERIFIED_TEXTURE`), after the
/// crosshairs'.
pub(crate) const VERIFIED_ICON: u32 = CROSSHAIR_ICON_FIRST + CROSSHAIR_ICON_CELLS;
/// First of the quick wheels' icon cells (`quick_wheel::ICONS`), after the verified
/// badge's.
pub(crate) const WHEEL_ICON_FIRST: u32 = VERIFIED_ICON + 1;
/// Quick wheel cells: three rows (the 44 icons of two boards, with room for four
/// more).
pub(crate) const WHEEL_ICON_CELLS: u32 = 3 * COLUMNS;
/// First of the settings menu's icon cells (`settings_icons::ICONS`), after the
/// quick wheels'.
pub(crate) const SETTINGS_ICON_FIRST: u32 = WHEEL_ICON_FIRST + WHEEL_ICON_CELLS;
/// Settings icon cells: two rows.
pub(crate) const SETTINGS_ICON_CELLS: u32 = 2 * COLUMNS;
/// First of the medals' small medallion cells (`medals::Medal::icon`), after the
/// settings menu's.
pub(crate) const MEDAL_ICON_FIRST: u32 = SETTINGS_ICON_FIRST + SETTINGS_ICON_CELLS;
/// Medal cells: room for twice the medals there are today.
pub(crate) const MEDAL_ICON_CELLS: u32 = 8;
/// First of the chat emojis' cells (`chat::emoji`), after the medals'.
/// Four emojis share a cell, one in each quarter.
pub(crate) const EMOJI_ICON_FIRST: u32 = MEDAL_ICON_FIRST + MEDAL_ICON_CELLS;
/// Emoji cells: four rows, room for EternalJK's 256 (`MAX_LOADABLE_EMOJIS`).
pub(crate) const EMOJI_ICON_CELLS: u32 = 4 * COLUMNS;
/// First of the players' pictures' cells (`avatars`), after the emojis'.
pub(crate) const AVATAR_ICON_FIRST: u32 = EMOJI_ICON_FIRST + EMOJI_ICON_CELLS;
/// Picture cells: two rows, the cache's pictures and the upload's preview.
pub(crate) const AVATAR_ICON_CELLS: u32 = 2 * COLUMNS;
/// The cell holding the riding vehicle's crosshair picture (`crosshairShader` of its
/// `.veh`), after the pictures'. One cell: it is replaced when the pilot changes vehicle.
pub(crate) const VEHICLE_CROSSHAIR_ICON: u32 = AVATAR_ICON_FIRST + AVATAR_ICON_CELLS;
/// First of the clan emblems' cells (`jof_tag::KNOWN`, in its order), after the vehicle
/// crosshair's.
pub(crate) const CLAN_ICON_FIRST: u32 = VEHICLE_CROSSHAIR_ICON + 1;
/// Clan emblem cells: room for four clans.
pub(crate) const CLAN_ICON_CELLS: u32 = 4;
/// First of the holocron tiers' cells (`holocrons::icons`), after the clan emblems'.
pub(crate) const HOLOCRON_ICON_FIRST: u32 = CLAN_ICON_FIRST + CLAN_ICON_CELLS;
/// Holocron cells: one per tier, with room for the same again.
pub(crate) const HOLOCRON_ICON_CELLS: u32 = 8;
/// First of the rocket lock-on's cells (`gfx/2d/wedge` and `gfx/2d/lock`,
/// `hud::rocket_lock`), after the holocrons'.
pub(crate) const ROCKET_LOCK_ICON_FIRST: u32 = HOLOCRON_ICON_FIRST + HOLOCRON_ICON_CELLS;
/// Rocket lock-on cells: the wedge and the lock.
pub(crate) const ROCKET_LOCK_ICON_CELLS: u32 = 2;
/// First of the emotes' picture cells (`emote_icons`), after the rocket lock-on's.
pub(crate) const EMOTE_ICON_FIRST: u32 = ROCKET_LOCK_ICON_FIRST + ROCKET_LOCK_ICON_CELLS;
/// Emote picture cells: two rows.
pub(crate) const EMOTE_ICON_CELLS: u32 = 2 * COLUMNS;
/// Every icon cell.
pub(crate) const ATLAS_CELLS: u32 = EMOTE_ICON_FIRST + EMOTE_ICON_CELLS;
const TOTAL_CELLS: u32 = ATLAS_CELLS;
const ATLAS_HEIGHT: u32 = TOTAL_CELLS.div_ceil(COLUMNS) * ICON_SIZE;

pub(super) struct IconAtlas {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

impl IconAtlas {
    pub(super) fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SJK retained UI icon atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::ui_target::TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SJK retained UI icon sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK retained UI icon atlas bind group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        Self {
            texture,
            bind_group,
        }
    }

    pub(super) fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub(super) fn upload(
        &self,
        queue: &crate::frame_queue::FrameQueue,
        texture: TextureId,
        rgba: &[u8],
    ) {
        if rgba.len() != (ICON_SIZE * ICON_SIZE * 4) as usize {
            return;
        }
        let index = texture.0.min(TOTAL_CELLS - 1);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: (index % COLUMNS) * ICON_SIZE,
                    y: (index / COLUMNS) * ICON_SIZE,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ICON_SIZE * 4),
                rows_per_image: Some(ICON_SIZE),
            },
            wgpu::Extent3d {
                width: ICON_SIZE,
                height: ICON_SIZE,
                depth_or_array_layers: 1,
            },
        );
    }
}

/// Atlas UV corners of `texture`; the map preview spans its own texture.
pub(super) fn uv_range(texture: TextureId) -> ([f32; 2], [f32; 2]) {
    if texture == super::LEVELSHOT_TEXTURE {
        return ([0.0, 0.0], [1.0, 1.0]);
    }
    let atlas = ATLAS_SIZE as f32;
    let index = texture.0.min(TOTAL_CELLS - 1);
    let x = (index % COLUMNS) * ICON_SIZE;
    let y = (index / COLUMNS) * ICON_SIZE;
    let uv = |x: u32, y: u32| [x as f32 / atlas, y as f32 / ATLAS_HEIGHT as f32];
    (uv(x, y), uv(x + ICON_SIZE, y + ICON_SIZE))
}

/// Push one textured quad sampling `uv` (top-left and bottom-right corners)
/// of whatever texture its draw run binds.
pub(super) fn push_quad(
    vertices: &mut Vec<ShapeVertex>,
    rect: Rect,
    (uv0, uv1): ([f32; 2], [f32; 2]),
    color: Color,
    opacity: f32,
    viewport: [f32; 2],
    capacity: usize,
) {
    push_quad_corners(
        vertices,
        rect,
        [uv0, [uv1[0], uv0[1]], uv1, [uv0[0], uv1[1]]],
        color,
        opacity,
        viewport,
        capacity,
    );
}

/// Push a textured quad with texture coordinates `uv` at its top-left,
/// top-right, bottom-right and bottom-left corners.
pub(super) fn push_quad_corners(
    vertices: &mut Vec<ShapeVertex>,
    rect: Rect,
    uv: [[f32; 2]; 4],
    color: Color,
    opacity: f32,
    viewport: [f32; 2],
    capacity: usize,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 || vertices.len() + 6 > capacity {
        return;
    }
    let position = |x: f32, y: f32| [x / viewport[0] * 2.0 - 1.0, 1.0 - y / viewport[1] * 2.0];
    let tint = [color.r, color.g, color.b, color.a * opacity];
    let [top_left, top_right, bottom_right, bottom_left] = uv;
    let points = [
        ([rect.x, rect.y], top_left),
        ([rect.right(), rect.y], top_right),
        ([rect.right(), rect.bottom()], bottom_right),
        ([rect.x, rect.y], top_left),
        ([rect.right(), rect.bottom()], bottom_right),
        ([rect.x, rect.bottom()], bottom_left),
    ];
    vertices.extend(points.map(|(pixel, uv)| ShapeVertex {
        position: position(pixel[0], pixel[1]),
        local: [0.0, 0.0],
        size: [rect.width, rect.height],
        start_color: tint,
        end_color: tint,
        parameters: [0.0, 2.0],
        uv,
    }));
}

/// The corners of `rect` (top-left, top-right, bottom-right, bottom-left) turned
/// `degrees` about `pivot`, clockwise on a screen whose y grows downwards, as
/// `RB_RotatePic` turns them.
pub(super) fn turned_corners(rect: Rect, pivot: [f32; 2], degrees: f32) -> [[f32; 2]; 4] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [
        [rect.x, rect.y],
        [rect.right(), rect.y],
        [rect.right(), rect.bottom()],
        [rect.x, rect.bottom()],
    ]
    .map(|[x, y]| {
        let (dx, dy) = (x - pivot[0], y - pivot[1]);
        [
            pivot[0] + dx * cos - dy * sin,
            pivot[1] + dx * sin + dy * cos,
        ]
    })
}

/// Push a textured quad whose corners (top-left, top-right, bottom-right,
/// bottom-left) are at `corners`, sampling `uv` (its top-left and bottom-right).
pub(super) fn push_quad_at(
    vertices: &mut Vec<ShapeVertex>,
    corners: [[f32; 2]; 4],
    (uv0, uv1): ([f32; 2], [f32; 2]),
    color: Color,
    opacity: f32,
    viewport: [f32; 2],
    capacity: usize,
) {
    if vertices.len() + 6 > capacity {
        return;
    }
    let position = |[x, y]: [f32; 2]| [x / viewport[0] * 2.0 - 1.0, 1.0 - y / viewport[1] * 2.0];
    let tint = [color.r, color.g, color.b, color.a * opacity];
    let uv = [uv0, [uv1[0], uv0[1]], uv1, [uv0[0], uv1[1]]];
    let width = (corners[1][0] - corners[0][0]).hypot(corners[1][1] - corners[0][1]);
    let height = (corners[3][0] - corners[0][0]).hypot(corners[3][1] - corners[0][1]);
    vertices.extend([0, 1, 2, 0, 2, 3].map(|corner| ShapeVertex {
        position: position(corners[corner]),
        local: [0.0, 0.0],
        size: [width, height],
        start_color: tint,
        end_color: tint,
        parameters: [0.0, 2.0],
        uv: uv[corner],
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `RB_RotatePic` turns about the given point: a quarter turn of a square to the
    /// left of its pivot puts it above the pivot.
    #[test]
    fn a_quarter_turn_moves_the_square_around_its_pivot() {
        let corners = turned_corners(Rect::new(-10.0, 0.0, 10.0, 10.0), [0.0, 0.0], 90.0);
        let expected = [[0.0, -10.0], [0.0, 0.0], [-10.0, 0.0], [-10.0, -10.0]];
        for (corner, expected) in corners.iter().zip(expected) {
            assert!((corner[0] - expected[0]).abs() < 1e-4, "{corners:?}");
            assert!((corner[1] - expected[1]).abs() < 1e-4, "{corners:?}");
        }
        let unturned = turned_corners(Rect::new(1.0, 2.0, 3.0, 4.0), [5.0, 5.0], 0.0);
        assert_eq!(unturned, [[1.0, 2.0], [4.0, 2.0], [4.0, 6.0], [1.0, 6.0]]);
    }

    #[test]
    fn the_atlas_fits_the_texture_size_the_client_asks_for() {
        // The device is created with wgpu's default limits (`gpu_context.rs`),
        // whose largest 2D texture is 8192 on a side; the atlas is taller than
        // it is wide.
        let limit = wgpu::Limits::default().max_texture_dimension_2d;
        assert!(super::ATLAS_HEIGHT <= limit, "{}", super::ATLAS_HEIGHT);
    }
}
