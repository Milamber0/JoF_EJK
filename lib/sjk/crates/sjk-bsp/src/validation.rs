//! Validated BSP validation helpers.
use super::*;

pub(super) fn validate_nodes(
    raw: Vec<RawNode>,
    plane_count: usize,
    leaf_count: usize,
) -> Result<Vec<Node>, BspError> {
    let node_count = raw.len();
    raw.into_iter()
        .enumerate()
        .map(|(index, node)| {
            let plane = validate_index(LumpKind::Nodes, index, node.plane, plane_count)?;
            let children = [
                decode_child(index, node.children[0], node_count, leaf_count)?,
                decode_child(index, node.children[1], node_count, leaf_count)?,
            ];
            Ok(Node {
                plane,
                children,
                minimums: node.minimums,
                maximums: node.maximums,
            })
        })
        .collect()
}

pub(super) fn decode_child(
    owner_index: usize,
    value: i32,
    node_count: usize,
    leaf_count: usize,
) -> Result<NodeChild, BspError> {
    if value >= 0 {
        return Ok(NodeChild::Node(validate_index(
            LumpKind::Nodes,
            owner_index,
            value,
            node_count,
        )?));
    }
    let leaf = value
        .checked_neg()
        .and_then(|value| value.checked_sub(1))
        .ok_or(BspError::InvalidReference {
            owner: LumpKind::Nodes,
            index: owner_index,
            target: value,
            target_count: leaf_count,
        })?;
    Ok(NodeChild::Leaf(validate_index(
        LumpKind::Nodes,
        owner_index,
        leaf,
        leaf_count,
    )?))
}

pub(super) fn validate_node_graph(nodes: &[Node]) -> Result<(), BspError> {
    let mut colors = vec![0_u8; nodes.len()];
    for root in 0..nodes.len() {
        if colors[root] != 0 {
            continue;
        }
        let mut stack = vec![(root, false)];
        while let Some((index, exiting)) = stack.pop() {
            if exiting {
                colors[index] = 2;
                continue;
            }
            match colors[index] {
                1 => return Err(BspError::NodeCycle(index)),
                2 => continue,
                _ => {}
            }
            colors[index] = 1;
            stack.push((index, true));
            for child in nodes[index].children.into_iter().rev() {
                if let NodeChild::Node(child) = child {
                    stack.push((child, false));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn validate_leaves(
    raw: Vec<RawLeaf>,
    leaf_surface_count: usize,
    leaf_brush_count: usize,
) -> Result<Vec<Leaf>, BspError> {
    raw.into_iter()
        .enumerate()
        .map(|(index, leaf)| {
            Ok(Leaf {
                cluster: leaf.cluster,
                area: leaf.area,
                minimums: leaf.minimums,
                maximums: leaf.maximums,
                leaf_surfaces: validate_range(
                    LumpKind::Leaves,
                    index,
                    leaf.first_leaf_surface,
                    leaf.leaf_surface_count,
                    leaf_surface_count,
                )?,
                leaf_brushes: validate_range(
                    LumpKind::Leaves,
                    index,
                    leaf.first_leaf_brush,
                    leaf.leaf_brush_count,
                    leaf_brush_count,
                )?,
            })
        })
        .collect()
}

pub(super) fn validate_leaf_brushes(indices: &[usize], brush_count: usize) -> Result<(), BspError> {
    for (index, target) in indices.iter().copied().enumerate() {
        if target >= brush_count {
            return Err(BspError::InvalidReference {
                owner: LumpKind::LeafBrushes,
                index,
                target: i32::try_from(target).unwrap_or(i32::MAX),
                target_count: brush_count,
            });
        }
    }
    Ok(())
}

pub(super) fn validate_leaf_surfaces(
    indices: &[usize],
    surface_count: usize,
) -> Result<(), BspError> {
    for (index, target) in indices.iter().copied().enumerate() {
        if target >= surface_count {
            return Err(BspError::InvalidReference {
                owner: LumpKind::LeafSurfaces,
                index,
                target: i32::try_from(target).unwrap_or(i32::MAX),
                target_count: surface_count,
            });
        }
    }
    Ok(())
}

pub(super) fn validate_brush_sides(
    sides: &[BrushSide],
    plane_count: usize,
    shader_count: usize,
) -> Result<(), BspError> {
    for (index, side) in sides.iter().enumerate() {
        validate_usize_index(LumpKind::BrushSides, index, side.plane, plane_count)?;
        validate_usize_index(LumpKind::BrushSides, index, side.shader, shader_count)?;
    }
    Ok(())
}

pub(super) fn validate_brushes(
    raw: Vec<RawBrush>,
    side_count: usize,
    shaders: &[Shader],
) -> Result<Vec<Brush>, BspError> {
    raw.into_iter()
        .enumerate()
        .map(|(index, brush)| {
            if brush.side_count < 6 {
                return Err(BspError::InvalidBrushSideCount {
                    index,
                    actual: brush.side_count,
                });
            }
            let shader = validate_index(LumpKind::Brushes, index, brush.shader, shaders.len())?;
            Ok(Brush {
                sides: validate_range(
                    LumpKind::Brushes,
                    index,
                    brush.first_side,
                    brush.side_count,
                    side_count,
                )?,
                shader,
                content_flags: shaders[shader].content_flags,
            })
        })
        .collect()
}

pub(super) fn validate_index(
    owner: LumpKind,
    index: usize,
    target: i32,
    target_count: usize,
) -> Result<usize, BspError> {
    let target_index = usize::try_from(target).map_err(|_| BspError::InvalidReference {
        owner,
        index,
        target,
        target_count,
    })?;
    validate_usize_index(owner, index, target_index, target_count)?;
    Ok(target_index)
}

pub(super) fn validate_usize_index(
    owner: LumpKind,
    index: usize,
    target: usize,
    target_count: usize,
) -> Result<(), BspError> {
    if target >= target_count {
        return Err(BspError::InvalidReference {
            owner,
            index,
            target: i32::try_from(target).unwrap_or(i32::MAX),
            target_count,
        });
    }
    Ok(())
}

pub(super) fn validate_range(
    owner: LumpKind,
    index: usize,
    first: i32,
    count: i32,
    target_count: usize,
) -> Result<Range<usize>, BspError> {
    let start = usize::try_from(first).map_err(|_| BspError::InvalidRange {
        owner,
        index,
        first,
        count,
        target_count,
    })?;
    let length = usize::try_from(count).map_err(|_| BspError::InvalidRange {
        owner,
        index,
        first,
        count,
        target_count,
    })?;
    let end = start
        .checked_add(length)
        .filter(|end| *end <= target_count)
        .ok_or(BspError::InvalidRange {
            owner,
            index,
            first,
            count,
            target_count,
        })?;
    Ok(start..end)
}

pub(super) fn require_nonempty(lump: LumpKind, count: usize) -> Result<(), BspError> {
    if count == 0 {
        return Err(BspError::EmptyRequiredLump(lump));
    }
    Ok(())
}
