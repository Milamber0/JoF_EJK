//! Parser tolerance tests on synthetic `.glm` files: one surface, one LOD, one triangle.

/// A mesh whose three vertices have one full weight on bone 0 unless `weights` says otherwise.
pub(crate) struct TestGlm<'a> {
    pub animation_name: &'a str,
    pub bone_count: i32,
    /// Global bones the surface references.
    pub bone_references: Vec<i32>,
    /// Per vertex: the packed weight/bone-index word and the four low weight bytes.
    pub weights: [(u32, [u8; 4]); 3],
}

impl Default for TestGlm<'_> {
    fn default() -> Self {
        Self {
            animation_name: "models/players/_humanoid/_humanoid",
            bone_count: 1,
            bone_references: vec![0],
            weights: [(0, [0; 4]); 3],
        }
    }
}

fn put_i32(data: &mut [u8], offset: usize, value: i32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_name(data: &mut [u8], offset: usize, name: &str) {
    data[offset..offset + name.len()].copy_from_slice(name.as_bytes());
}

impl TestGlm<'_> {
    pub(crate) fn bytes(&self) -> Vec<u8> {
        const HEADER: usize = 164;
        const HIERARCHY: usize = HEADER + 4;
        const LOD: usize = HIERARCHY + 144;
        const SURFACE: usize = LOD + 8;
        const VERTICES: usize = 40;
        let triangles = VERTICES + 3 * 32 + 3 * 8;
        let references = triangles + 12;
        let surface_end = references + 4 * self.bone_references.len();
        let end = SURFACE + surface_end;
        let mut data = vec![0; end];
        data[..4].copy_from_slice(b"2LGM");
        put_i32(&mut data, 4, 6);
        put_name(&mut data, 8, "model");
        put_name(&mut data, 72, self.animation_name);
        put_i32(&mut data, 140, self.bone_count);
        put_i32(&mut data, 144, 1);
        put_i32(&mut data, 148, LOD as i32);
        put_i32(&mut data, 152, 1);
        put_i32(&mut data, 156, HIERARCHY as i32);
        put_i32(&mut data, 160, end as i32);
        put_i32(&mut data, HEADER, (HIERARCHY - HEADER) as i32);
        put_name(&mut data, HIERARCHY, "torso");
        put_i32(&mut data, HIERARCHY + 136, -1);
        put_i32(&mut data, LOD, (end - LOD) as i32);
        put_i32(&mut data, LOD + 4, (SURFACE - (LOD + 4)) as i32);
        put_i32(&mut data, SURFACE + 12, 3);
        put_i32(&mut data, SURFACE + 16, VERTICES as i32);
        put_i32(&mut data, SURFACE + 20, 1);
        put_i32(&mut data, SURFACE + 24, triangles as i32);
        put_i32(&mut data, SURFACE + 28, self.bone_references.len() as i32);
        put_i32(&mut data, SURFACE + 32, references as i32);
        put_i32(&mut data, SURFACE + 36, surface_end as i32);
        for (index, (packed, low)) in self.weights.iter().enumerate() {
            let vertex = SURFACE + VERTICES + index * 32;
            data[vertex + 12..vertex + 16].copy_from_slice(&(index as f32).to_le_bytes());
            data[vertex + 24..vertex + 28].copy_from_slice(&packed.to_le_bytes());
            data[vertex + 28..vertex + 32].copy_from_slice(low);
        }
        for (corner, vertex) in [0, 1, 2].into_iter().enumerate() {
            put_i32(&mut data, SURFACE + triangles + corner * 4, vertex);
        }
        for (index, bone) in self.bone_references.iter().enumerate() {
            put_i32(&mut data, SURFACE + references + index * 4, *bone);
        }
        data
    }
}

#[test]
fn the_fixture_parses() {
    let glm = crate::Glm::parse(&TestGlm::default().bytes()).expect("parse");
    assert_eq!(glm.animation_name, "models/players/_humanoid/_humanoid");
    assert_eq!(glm.lods[0].surfaces[0].vertices.len(), 3);
    assert_eq!(glm.lods[0].surfaces[0].vertices[0].weights[0].weight, 1.0);
}

#[test]
fn weights_adding_up_past_one_are_kept_as_written() {
    // Three weights; the first two are both 1023/1023, so the last is 1 - 2.
    let packed = (2 << 30) | (0b1111 << 20);
    let glm = crate::Glm::parse(
        &TestGlm {
            weights: [(packed, [0xff, 0xff, 0, 0]); 3],
            ..TestGlm::default()
        }
        .bytes(),
    )
    .expect("rd-vanilla loads such a mesh");
    let weights = &glm.lods[0].surfaces[0].vertices[0].weights;
    assert_eq!(weights.len(), 3);
    assert_eq!(weights[0].weight, 1.0);
    assert_eq!(weights[1].weight, 1.0);
    assert_eq!(weights[2].weight, -1.0);
}

#[test]
fn a_leading_slash_on_the_skeleton_name_is_dropped() {
    for name in ["/models/players/v-19/v-19", r"\models/players/v-19/v-19"] {
        let glm = crate::Glm::parse(
            &TestGlm {
                animation_name: name,
                ..TestGlm::default()
            }
            .bytes(),
        )
        .expect("parse");
        assert_eq!(glm.animation_name, "models/players/v-19/v-19");
    }
}
