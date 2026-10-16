//! A reader for skinned glTF 2.0 models (`.gltf` with external buffers, or `.glb`):
//! the first skin's joints in their bind and rest poses, every triangle primitive of
//! the meshes that skin deforms, and the animations' channels that move those
//! joints. It reads what a player model needs and nothing else: no morph targets,
//! cameras or extensions. Images stay URIs for the caller to resolve; images
//! embedded in a buffer are not supported.
//!
//! Matrices are glTF's own space (metres, +Y up); converting to a game's axes and
//! units is the caller's job.

use serde_json::Value;
use std::fmt;

/// A skinned model: its joints and the primitives they deform.
#[derive(Clone, Debug, Default)]
pub struct GltfModel {
    pub joints: Vec<GltfJoint>,
    pub primitives: Vec<GltfPrimitive>,
    pub animations: Vec<GltfAnimation>,
}

/// One animation: its channels that move skin joints (others are left out).
#[derive(Clone, Debug, Default)]
pub struct GltfAnimation {
    pub name: String,
    pub channels: Vec<GltfChannel>,
}

/// One joint's translation, rotation or scale over time, in its parent's space
/// (the glTF node's local transform).
#[derive(Clone, Debug)]
pub struct GltfChannel {
    /// The joint's index in [`GltfModel::joints`].
    pub joint: usize,
    pub path: GltfPath,
    /// Key times in seconds, rising.
    pub times: Vec<f32>,
    /// One value per key: `[x, y, z, 0]` for translation and scale, the quaternion
    /// `[x, y, z, w]` for rotation.
    pub values: Vec<[f32; 4]>,
    /// `STEP` interpolation (hold each key); otherwise linear. Cubic splines keep
    /// their keys' values and are played linear.
    pub step: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GltfPath {
    Translation,
    Rotation,
    Scale,
}

/// One skin joint. `bind` is its model-space bind transform (the inverse of its
/// inverse bind matrix), row-major 3x4 like the GLA matrices.
#[derive(Clone, Debug)]
pub struct GltfJoint {
    pub name: String,
    /// The nearest ancestor node that is also a joint of the skin.
    pub parent: Option<usize>,
    pub bind: [[f32; 4]; 3],
    pub inverse_bind: [[f32; 4]; 3],
    /// The node's own transform relative to its parent node (its rest pose), the
    /// space the animations' channels are in.
    pub rest: [[f32; 4]; 3],
}

/// One triangle list with up to four joint influences per vertex (the strongest
/// four of up to eight, renormalised).
#[derive(Clone, Debug, Default)]
pub struct GltfPrimitive {
    /// The material's name, if it has one.
    pub material: Option<String>,
    /// The URI of the base colour texture's image, relative to the model file.
    pub base_color_image: Option<String>,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub texture_coordinates: Vec<[f32; 2]>,
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

/// Why a glTF model could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GltfError(pub String);

impl fmt::Display for GltfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "glTF: {}", self.0)
    }
}

impl std::error::Error for GltfError {}

fn error(message: impl Into<String>) -> GltfError {
    GltfError(message.into())
}

const GLB_MAGIC: u32 = 0x4654_6C67;
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;

/// Read a `.gltf` (JSON) or `.glb` model. `resolve` returns the bytes of a URI
/// relative to the model file (a `.bin` buffer); `None` makes the load fail.
pub fn parse(
    bytes: &[u8],
    resolve: impl Fn(&str) -> Option<Vec<u8>>,
) -> Result<GltfModel, GltfError> {
    let (json, binary) = split_glb(bytes)?;
    let document: Value =
        serde_json::from_slice(json).map_err(|e| error(format!("bad JSON: {e}")))?;
    let buffers = array(&document, "buffers")
        .iter()
        .enumerate()
        .map(
            |(index, buffer)| match buffer.get("uri").and_then(Value::as_str) {
                Some(uri) if uri.starts_with("data:") => Err(error(format!(
                    "buffer {index}: data URIs are not supported"
                ))),
                Some(uri) => resolve(&percent_decode(uri))
                    .ok_or_else(|| error(format!("buffer {index}: {uri:?} not found"))),
                None => binary
                    .map(<[u8]>::to_vec)
                    .ok_or_else(|| error(format!("buffer {index} has no URI and no GLB chunk"))),
            },
        )
        .collect::<Result<Vec<_>, _>>()?;
    let reader = Reader {
        document: &document,
        buffers,
    };
    reader.model()
}

fn split_glb(bytes: &[u8]) -> Result<(&[u8], Option<&[u8]>), GltfError> {
    let word = |offset: usize| -> Option<u32> {
        Some(u32::from_le_bytes(
            bytes.get(offset..offset + 4)?.try_into().ok()?,
        ))
    };
    if word(0) != Some(GLB_MAGIC) {
        return Ok((bytes, None));
    }
    let (mut json, mut binary) = (None, None);
    let mut offset = 12;
    while let (Some(length), Some(kind)) = (word(offset), word(offset + 4)) {
        let start = offset + 8;
        let end = start + length as usize;
        let chunk = bytes
            .get(start..end)
            .ok_or_else(|| error("GLB chunk runs past the file"))?;
        match kind {
            CHUNK_JSON => json = Some(chunk),
            CHUNK_BIN => binary = Some(chunk),
            _ => {}
        }
        offset = end;
    }
    Ok((
        json.ok_or_else(|| error("GLB without a JSON chunk"))?,
        binary,
    ))
}

/// `%20` and the like in a URI; anything else is kept as written.
fn percent_decode(uri: &str) -> String {
    let bytes = uri.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(value) = uri
                .get(index + 1..index + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(value);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn index(value: &Value, key: &str) -> Option<usize> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|i| usize::try_from(i).ok())
}

/// Column-major 4x4, as glTF stores it.
type Mat4 = [f32; 16];

const IDENTITY: Mat4 = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn rows(matrix: &Mat4) -> [[f32; 4]; 3] {
    let at = |row: usize, column: usize| matrix[column * 4 + row];
    [
        [at(0, 0), at(0, 1), at(0, 2), at(0, 3)],
        [at(1, 0), at(1, 1), at(1, 2), at(1, 3)],
        [at(2, 0), at(2, 1), at(2, 2), at(2, 3)],
    ]
}

/// Inverse of an affine transform (rotation, scale and translation; no projection).
fn invert_affine(m: &Mat4) -> Option<Mat4> {
    let a = |row: usize, column: usize| m[column * 4 + row];
    let det = a(0, 0) * (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1))
        - a(0, 1) * (a(1, 0) * a(2, 2) - a(1, 2) * a(2, 0))
        + a(0, 2) * (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0));
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = |row: usize, column: usize| -> f32 {
        let (r0, r1) = match column {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        let (c0, c1) = match row {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        let minor = a(r0, c0) * a(r1, c1) - a(r0, c1) * a(r1, c0);
        let sign = if (row + column).is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        sign * minor / det
    };
    let mut out = IDENTITY;
    for row in 0..3 {
        for column in 0..3 {
            out[column * 4 + row] = inv(row, column);
        }
    }
    for row in 0..3 {
        out[12 + row] = -(0..3).map(|k| out[k * 4 + row] * m[12 + k]).sum::<f32>();
    }
    Some(out)
}

/// A node's local transform: its `matrix`, or its translation, rotation and scale.
fn local_matrix(node: &Value) -> Mat4 {
    let numbers = |key: &str| -> Option<Vec<f32>> {
        node.get(key)?
            .as_array()?
            .iter()
            .map(|v| v.as_f64().map(|v| v as f32))
            .collect()
    };
    if let Some(matrix) = numbers("matrix").filter(|m| m.len() == 16) {
        return matrix.try_into().expect("16 numbers");
    }
    let t = numbers("translation")
        .filter(|t| t.len() == 3)
        .unwrap_or_else(|| vec![0.0; 3]);
    let q = numbers("rotation")
        .filter(|q| q.len() == 4)
        .unwrap_or_else(|| vec![0.0, 0.0, 0.0, 1.0]);
    let s = numbers("scale")
        .filter(|s| s.len() == 3)
        .unwrap_or_else(|| vec![1.0; 3]);
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let r = [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ];
    let mut m = IDENTITY;
    for row in 0..3 {
        for column in 0..3 {
            m[column * 4 + row] = r[row][column] * s[column];
        }
        m[12 + row] = t[row];
    }
    m
}

struct Reader<'a> {
    document: &'a Value,
    buffers: Vec<Vec<u8>>,
}

impl Reader<'_> {
    fn model(&self) -> Result<GltfModel, GltfError> {
        let nodes = array(self.document, "nodes");
        let skin = array(self.document, "skins")
            .first()
            .ok_or_else(|| error("the model has no skin"))?;
        let joint_nodes: Vec<usize> = array(skin, "joints")
            .iter()
            .map(|joint| joint.as_u64().map(|i| i as usize))
            .collect::<Option<_>>()
            .ok_or_else(|| error("a skin joint is not a node index"))?;
        if joint_nodes.is_empty() || joint_nodes.iter().any(|&n| n >= nodes.len()) {
            return Err(error("the skin's joints are empty or name missing nodes"));
        }
        let inverse_binds: Vec<Mat4> = match index(skin, "inverseBindMatrices") {
            Some(accessor) => self
                .floats(accessor, 16)?
                .chunks_exact(16)
                .map(|chunk| chunk.try_into().expect("16 floats"))
                .collect(),
            None => vec![IDENTITY; joint_nodes.len()],
        };
        if inverse_binds.len() < joint_nodes.len() {
            return Err(error("fewer inverse bind matrices than joints"));
        }
        let mut node_parent = vec![None; nodes.len()];
        for (parent, node) in nodes.iter().enumerate() {
            for child in array(node, "children") {
                if let Some(child) = child.as_u64().map(|c| c as usize)
                    && child < nodes.len()
                {
                    node_parent[child] = Some(parent);
                }
            }
        }
        let joint_of_node = |node: usize| joint_nodes.iter().position(|&j| j == node);
        let joints = joint_nodes
            .iter()
            .zip(&inverse_binds)
            .map(|(&node, inverse_bind)| {
                let mut parent = node_parent[node];
                let mut guard = 0;
                while let Some(candidate) = parent {
                    if joint_of_node(candidate).is_some() || guard > nodes.len() {
                        break;
                    }
                    parent = node_parent[candidate];
                    guard += 1;
                }
                let bind = invert_affine(inverse_bind)
                    .ok_or_else(|| error("a joint's inverse bind matrix is singular"))?;
                Ok(GltfJoint {
                    name: nodes[node]
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    parent: parent.and_then(joint_of_node),
                    bind: rows(&bind),
                    inverse_bind: rows(inverse_bind),
                    rest: rows(&local_matrix(&nodes[node])),
                })
            })
            .collect::<Result<Vec<_>, GltfError>>()?;
        // The meshes this skin deforms: nodes holding both a mesh and skin 0.
        let mut primitives = Vec::new();
        for node in nodes {
            if index(node, "skin") != Some(0) {
                continue;
            }
            let Some(mesh) =
                index(node, "mesh").and_then(|m| array(self.document, "meshes").get(m))
            else {
                continue;
            };
            for primitive in array(mesh, "primitives") {
                if index(primitive, "mode").is_some_and(|mode| mode != 4) {
                    continue;
                }
                primitives.push(self.primitive(primitive)?);
            }
        }
        if primitives.is_empty() {
            return Err(error("no triangle mesh uses the skin"));
        }
        let animations = array(self.document, "animations")
            .iter()
            .enumerate()
            .map(|(number, animation)| self.animation(number, animation, &joint_of_node))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(GltfModel {
            joints,
            primitives,
            animations,
        })
    }

    fn animation(
        &self,
        number: usize,
        animation: &Value,
        joint_of_node: &impl Fn(usize) -> Option<usize>,
    ) -> Result<GltfAnimation, GltfError> {
        let name = animation
            .get("name")
            .and_then(Value::as_str)
            .map_or_else(|| format!("animation_{number}"), str::to_owned);
        let samplers = array(animation, "samplers");
        let mut channels = Vec::new();
        for channel in array(animation, "channels") {
            let Some(target) = channel.get("target") else {
                continue;
            };
            let Some(joint) = index(target, "node").and_then(joint_of_node) else {
                continue;
            };
            let path = match target.get("path").and_then(Value::as_str) {
                Some("translation") => GltfPath::Translation,
                Some("rotation") => GltfPath::Rotation,
                Some("scale") => GltfPath::Scale,
                _ => continue,
            };
            let sampler = index(channel, "sampler")
                .and_then(|s| samplers.get(s))
                .ok_or_else(|| error(format!("animation {name:?}: a channel has no sampler")))?;
            let (Some(input), Some(output)) = (index(sampler, "input"), index(sampler, "output"))
            else {
                return Err(error(format!("animation {name:?}: a sampler lacks data")));
            };
            let interpolation = sampler
                .get("interpolation")
                .and_then(Value::as_str)
                .unwrap_or("LINEAR");
            let times = self.floats(input, 1)?;
            let width = if path == GltfPath::Rotation { 4 } else { 3 };
            let mut values: Vec<[f32; 4]> = self
                .floats(output, width)?
                .chunks_exact(width)
                .map(|c| [c[0], c[1], c[2], c.get(3).copied().unwrap_or(0.0)])
                .collect();
            if interpolation == "CUBICSPLINE" {
                // In-tangent, value, out-tangent per key: keep the values.
                values = values.chunks_exact(3).map(|key| key[1]).collect();
            }
            if times.is_empty() || values.len() != times.len() {
                return Err(error(format!(
                    "animation {name:?}: {} keys for {} times",
                    values.len(),
                    times.len()
                )));
            }
            if times.windows(2).any(|pair| pair[1] < pair[0]) {
                return Err(error(format!("animation {name:?}: key times go back")));
            }
            channels.push(GltfChannel {
                joint,
                path,
                times,
                values,
                step: interpolation == "STEP",
            });
        }
        Ok(GltfAnimation { name, channels })
    }

    fn primitive(&self, primitive: &Value) -> Result<GltfPrimitive, GltfError> {
        let attributes = primitive
            .get("attributes")
            .ok_or_else(|| error("a primitive has no attributes"))?;
        let attribute = |name: &str| index(attributes, name);
        let positions = self.vec3(attribute("POSITION").ok_or_else(|| error("no POSITION"))?)?;
        let count = positions.len();
        let normals = match attribute("NORMAL") {
            Some(accessor) => self.vec3(accessor)?,
            None => vec![[0.0, 1.0, 0.0]; count],
        };
        let texture_coordinates = match attribute("TEXCOORD_0") {
            Some(accessor) => self
                .floats(accessor, 2)?
                .chunks_exact(2)
                .map(|c| [c[0], c[1]])
                .collect(),
            None => vec![[0.0; 2]; count],
        };
        let mut influences: Vec<Vec<(u16, f32)>> = vec![Vec::with_capacity(8); count];
        for set in 0..2 {
            let (Some(joints), Some(weights)) = (
                attribute(&format!("JOINTS_{set}")),
                attribute(&format!("WEIGHTS_{set}")),
            ) else {
                continue;
            };
            let joints = self.integers(joints, 4)?;
            let weights = self.floats(weights, 4)?;
            for (vertex, list) in influences.iter_mut().enumerate() {
                for slot in 0..4 {
                    let (Some(&joint), Some(&weight)) = (
                        joints.get(vertex * 4 + slot),
                        weights.get(vertex * 4 + slot),
                    ) else {
                        continue;
                    };
                    if weight > 0.0 {
                        list.push((u16::try_from(joint).unwrap_or(0), weight));
                    }
                }
            }
        }
        let mut joints = Vec::with_capacity(count);
        let mut weights = Vec::with_capacity(count);
        for mut list in influences {
            list.sort_by(|a, b| b.1.total_cmp(&a.1));
            list.truncate(4);
            let total: f32 = list.iter().map(|(_, w)| w).sum();
            let mut j = [0u16; 4];
            let mut w = [0.0f32; 4];
            for (slot, (joint, weight)) in list.into_iter().enumerate() {
                j[slot] = joint;
                w[slot] = if total > 0.0 { weight / total } else { 0.0 };
            }
            if total <= 0.0 {
                w[0] = 1.0;
            }
            joints.push(j);
            weights.push(w);
        }
        let indices = match index(primitive, "indices") {
            Some(accessor) => self.integers(accessor, 1)?,
            None => (0..count as u32).collect(),
        };
        if indices.iter().any(|&i| i as usize >= count) || !indices.len().is_multiple_of(3) {
            return Err(error("a primitive's indices are out of range"));
        }
        let material =
            index(primitive, "material").and_then(|m| array(self.document, "materials").get(m));
        let base_color_image = material
            .and_then(|m| m.get("pbrMetallicRoughness"))
            .and_then(|pbr| pbr.get("baseColorTexture"))
            .and_then(|texture| index(texture, "index"))
            .and_then(|t| array(self.document, "textures").get(t))
            .and_then(|texture| index(texture, "source"))
            .and_then(|i| array(self.document, "images").get(i))
            .and_then(|image| image.get("uri"))
            .and_then(Value::as_str)
            .map(percent_decode);
        Ok(GltfPrimitive {
            material: material
                .and_then(|m| m.get("name"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            base_color_image,
            positions,
            normals,
            texture_coordinates,
            joints,
            weights,
            indices,
        })
    }

    fn vec3(&self, accessor: usize) -> Result<Vec<[f32; 3]>, GltfError> {
        Ok(self
            .floats(accessor, 3)?
            .chunks_exact(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect())
    }

    /// The accessor's elements as raw component values, `width` per element.
    fn components(&self, accessor: usize, width: usize) -> Result<Vec<f64>, GltfError> {
        let accessor_value = array(self.document, "accessors")
            .get(accessor)
            .ok_or_else(|| error(format!("accessor {accessor} is missing")))?;
        let count = index(accessor_value, "count").unwrap_or(0);
        let component = index(accessor_value, "componentType").unwrap_or(0);
        let normalized = accessor_value
            .get("normalized")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let size = match component {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            other => {
                return Err(error(format!(
                    "accessor {accessor}: component type {other}"
                )));
            }
        };
        let Some(view) = index(accessor_value, "bufferView") else {
            return Ok(vec![0.0; count * width]);
        };
        let view = array(self.document, "bufferViews")
            .get(view)
            .ok_or_else(|| error(format!("accessor {accessor}: missing buffer view")))?;
        let buffer = index(view, "buffer")
            .and_then(|b| self.buffers.get(b))
            .ok_or_else(|| error(format!("accessor {accessor}: missing buffer")))?;
        let start = index(view, "byteOffset").unwrap_or(0)
            + index(accessor_value, "byteOffset").unwrap_or(0);
        let stride = index(view, "byteStride").unwrap_or(size * width);
        let mut out = Vec::with_capacity(count * width);
        for element in 0..count {
            for slot in 0..width {
                let at = start + element * stride + slot * size;
                let bytes = buffer
                    .get(at..at + size)
                    .ok_or_else(|| error(format!("accessor {accessor} runs past its buffer")))?;
                let value = match component {
                    5120 => {
                        let v = f64::from(bytes[0] as i8);
                        if normalized { (v / 127.0).max(-1.0) } else { v }
                    }
                    5121 => {
                        let v = f64::from(bytes[0]);
                        if normalized { v / 255.0 } else { v }
                    }
                    5122 => {
                        let v = f64::from(i16::from_le_bytes([bytes[0], bytes[1]]));
                        if normalized {
                            (v / 32767.0).max(-1.0)
                        } else {
                            v
                        }
                    }
                    5123 => {
                        let v = f64::from(u16::from_le_bytes([bytes[0], bytes[1]]));
                        if normalized { v / 65535.0 } else { v }
                    }
                    5125 => f64::from(u32::from_le_bytes(bytes.try_into().expect("4 bytes"))),
                    _ => f64::from(f32::from_le_bytes(bytes.try_into().expect("4 bytes"))),
                };
                out.push(value);
            }
        }
        Ok(out)
    }

    fn floats(&self, accessor: usize, width: usize) -> Result<Vec<f32>, GltfError> {
        Ok(self
            .components(accessor, width)?
            .into_iter()
            .map(|v| v as f32)
            .collect())
    }

    fn integers(&self, accessor: usize, width: usize) -> Result<Vec<u32>, GltfError> {
        Ok(self
            .components(accessor, width)?
            .into_iter()
            .map(|v| v.max(0.0) as u32)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two joints (root at the origin, child one metre up) and one triangle, its
    /// first vertex bound to the child.
    fn tiny() -> (String, Vec<u8>) {
        let mut bin = Vec::new();
        let floats = |bin: &mut Vec<u8>, values: &[f32]| {
            for v in values {
                bin.extend_from_slice(&v.to_le_bytes());
            }
        };
        // positions (36 bytes)
        floats(&mut bin, &[0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
        // joints: u8 x4 x3 (12 bytes)
        bin.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0]);
        // weights (48 bytes)
        floats(
            &mut bin,
            &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0],
        );
        // inverse binds (128 bytes): identity, then translate(0,-1,0)
        floats(
            &mut bin,
            &[
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
        );
        floats(
            &mut bin,
            &[
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 1.0,
            ],
        );
        let json = r#"{
            "asset": {"version": "2.0"},
            "buffers": [{"uri": "tiny.bin", "byteLength": 224}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 36},
                {"buffer": 0, "byteOffset": 36, "byteLength": 12},
                {"buffer": 0, "byteOffset": 48, "byteLength": 48},
                {"buffer": 0, "byteOffset": 96, "byteLength": 128}
            ],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 1, "componentType": 5121, "count": 3, "type": "VEC4"},
                {"bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC4"},
                {"bufferView": 3, "componentType": 5126, "count": 2, "type": "MAT4"}
            ],
            "images": [{"uri": "body%20skin.png"}],
            "textures": [{"source": 0}],
            "materials": [{"name": "Body", "pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "JOINTS_0": 1, "WEIGHTS_0": 2}, "material": 0}]}],
            "skins": [{"joints": [1, 2], "inverseBindMatrices": 3}],
            "nodes": [
                {"name": "Armature", "children": [1, 3]},
                {"name": "root", "children": [2]},
                {"name": "spine", "translation": [0, 1, 0]},
                {"name": "Body", "mesh": 0, "skin": 0}
            ]
        }"#;
        (json.to_owned(), bin)
    }

    #[test]
    fn a_skinned_triangle_reads_its_joints_bind_pose_and_weights() {
        let (json, bin) = tiny();
        let model = parse(json.as_bytes(), |uri| {
            (uri == "tiny.bin").then(|| bin.clone())
        })
        .expect("the model");
        assert_eq!(model.joints.len(), 2);
        assert_eq!(model.joints[0].name, "root");
        assert_eq!(model.joints[0].parent, None);
        assert_eq!(model.joints[1].parent, Some(0));
        // The child's bind pose is one metre up.
        assert!((model.joints[1].bind[1][3] - 1.0).abs() < 1e-6);
        let primitive = &model.primitives[0];
        assert_eq!(primitive.indices, vec![0, 1, 2]);
        assert_eq!(primitive.joints[0][0], 1);
        assert_eq!(primitive.weights[2], [0.5, 0.5, 0.0, 0.0]);
        assert_eq!(primitive.base_color_image.as_deref(), Some("body skin.png"));
        assert_eq!(primitive.material.as_deref(), Some("Body"));
    }

    #[test]
    fn an_inverse_bind_with_rotation_and_scale_inverts() {
        // 90 degrees about Z, scale 2, translated (1, 2, 3); column-major.
        let m: Mat4 = [
            0.0, 2.0, 0.0, 0.0, -2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 1.0, 2.0, 3.0, 1.0,
        ];
        let inverse = invert_affine(&m).expect("invertible");
        let point = [0.5f32, -1.0, 4.0];
        let apply = |m: &Mat4, p: [f32; 3]| -> [f32; 3] {
            std::array::from_fn(|row| {
                (0..3).map(|k| m[k * 4 + row] * p[k]).sum::<f32>() + m[12 + row]
            })
        };
        let back = apply(&inverse, apply(&m, point));
        for (a, b) in back.iter().zip(point) {
            assert!((a - b).abs() < 1e-5, "{back:?}");
        }
    }

    #[test]
    fn rest_poses_come_from_the_nodes() {
        let (json, bin) = tiny();
        let model = parse(json.as_bytes(), |_| Some(bin.clone())).expect("the model");
        assert_eq!(model.joints[1].rest[1][3], 1.0);
        assert_eq!(model.joints[1].rest[0][0], 1.0);
        assert!(model.animations.is_empty());
        // A quarter turn about +Z, as a node rotation, turns +X onto +Y.
        let node: Value = serde_json::from_str(
            r#"{"rotation": [0, 0, 0.70710677, 0.70710677], "translation": [1, 2, 3]}"#,
        )
        .unwrap();
        let m = rows(&local_matrix(&node));
        assert!(
            (m[1][0] - 1.0).abs() < 1e-6 && m[0][0].abs() < 1e-6,
            "{m:?}"
        );
        assert_eq!([m[0][3], m[1][3], m[2][3]], [1.0, 2.0, 3.0]);
    }

    #[test]
    fn an_animation_reads_the_channels_of_skin_joints() {
        let (json, mut bin) = tiny();
        // times (8 bytes) at 224, rotations (32 bytes) at 232, translations of the
        // armature node (24 bytes, not a joint) at 264.
        for value in [
            0.0f32, 0.5, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.70710677, 0.70710677,
        ] {
            bin.extend_from_slice(&value.to_le_bytes());
        }
        for value in [0.0f32; 6] {
            bin.extend_from_slice(&value.to_le_bytes());
        }
        let json = json
            .replace(r#""byteLength": 224}"#, r#""byteLength": 288}"#)
            .replace(
                r#"{"buffer": 0, "byteOffset": 96, "byteLength": 128}"#,
                r#"{"buffer": 0, "byteOffset": 96, "byteLength": 128},
                {"buffer": 0, "byteOffset": 224, "byteLength": 8},
                {"buffer": 0, "byteOffset": 232, "byteLength": 32},
                {"buffer": 0, "byteOffset": 264, "byteLength": 24}"#,
            )
            .replace(
                r#"{"bufferView": 3, "componentType": 5126, "count": 2, "type": "MAT4"}"#,
                r#"{"bufferView": 3, "componentType": 5126, "count": 2, "type": "MAT4"},
                {"bufferView": 4, "componentType": 5126, "count": 2, "type": "SCALAR"},
                {"bufferView": 5, "componentType": 5126, "count": 2, "type": "VEC4"},
                {"bufferView": 6, "componentType": 5126, "count": 2, "type": "VEC3"}"#,
            )
            .replace(
                r#""skins":"#,
                r#""animations": [{"name": "Wave", "samplers": [
                    {"input": 4, "output": 5, "interpolation": "STEP"},
                    {"input": 4, "output": 6}],
                    "channels": [
                    {"sampler": 0, "target": {"node": 2, "path": "rotation"}},
                    {"sampler": 1, "target": {"node": 0, "path": "translation"}}]}],
                "skins":"#,
            );
        let model = parse(json.as_bytes(), |_| Some(bin.clone())).expect("the model");
        let animation = &model.animations[0];
        assert_eq!(animation.name, "Wave");
        // The armature node is not a joint: its channel is left out.
        assert_eq!(animation.channels.len(), 1);
        let channel = &animation.channels[0];
        assert_eq!(
            (channel.joint, channel.path, channel.step),
            (1, GltfPath::Rotation, true)
        );
        assert_eq!(channel.times, vec![0.0, 0.5]);
        assert_eq!(channel.values[1], [0.0, 0.0, 0.70710677, 0.70710677]);
    }

    #[test]
    fn a_missing_buffer_is_an_error_not_a_panic() {
        let (json, _) = tiny();
        let error = parse(json.as_bytes(), |_| None).unwrap_err();
        assert!(error.0.contains("not found"), "{error}");
    }

    #[test]
    fn a_glb_reads_its_binary_chunk() {
        let (json, bin) = tiny();
        let json = json.replace(r#""uri": "tiny.bin", "#, "");
        let mut json = json.into_bytes();
        while json.len() % 4 != 0 {
            json.push(b' ');
        }
        let mut glb = Vec::new();
        glb.extend_from_slice(&GLB_MAGIC.to_le_bytes());
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&((12 + 8 + json.len() + 8 + bin.len()) as u32).to_le_bytes());
        glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
        glb.extend_from_slice(&CHUNK_JSON.to_le_bytes());
        glb.extend_from_slice(&json);
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(&CHUNK_BIN.to_le_bytes());
        glb.extend_from_slice(&bin);
        let model = parse(&glb, |_| None).expect("the GLB");
        assert_eq!(model.primitives[0].positions.len(), 3);
    }
}
