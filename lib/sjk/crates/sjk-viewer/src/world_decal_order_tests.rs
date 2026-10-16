//! Which world materials draw before the opaque world's fog pass (`underlay`).

use super::{SORT_DECAL, decal_underlay};
use sjk_shader::parse_shader_script;

/// `textures/decals/mp_imperial_symbol` (`mp/ctf4`, Sol's note of 07/10/2026): a
/// `polygonOffset` decal multiplying its wall, no depth written.
const IMPERIAL_SYMBOL: &str = "textures/decals/mp_imperial_symbol
{
	polygonOffset
	q3map_nolightmap
    {
        map textures/decals/mp_imperial_symbol
        alphaFunc GE128
        blendFunc GL_DST_COLOR GL_SRC_COLOR
    }
}
";

#[test]
fn a_polygon_offset_decal_draws_under_the_fog() {
    let shaders = parse_shader_script(IMPERIAL_SYMBOL.as_bytes(), "shaders/decals.shader")
        .expect("the decal's shader");
    let decal = &shaders[0];
    assert_eq!(decal.resolved_sort(), SORT_DECAL);
    assert!(decal_underlay(
        true,
        decal.resolved_sort(),
        decal.stages.iter().map(|stage| stage.depth_write)
    ));
}

#[test]
fn only_blended_depthless_decals_draw_under_the_fog() {
    // A decal stage that writes depth keeps its own `Equal` fog pass, which meets it.
    assert!(!decal_underlay(true, SORT_DECAL, [false, true]));
    // Opaque surfaces and later blends keep their places.
    assert!(!decal_underlay(false, 3.0, [false]));
    assert!(!decal_underlay(true, 5.0, [false]));
    assert!(!decal_underlay(true, 9.0, [false]));
    assert!(decal_underlay(true, SORT_DECAL, [false, false]));
}
