//! DX12 portability checks for the world programs, without a GPU.
//!
//! Where no `dxcompiler.dll` is present, wgpu's DX12 backend compiles HLSL with FXC.
//! FXC cannot assign a vector or matrix component through a runtime index; it unrolls
//! the loops around such a store instead (warning X3550). That works while every
//! enclosing loop runs a constant number of times. With a runtime trip count anywhere
//! around it (naga emits every loop with an iteration guard, so a constant inner loop
//! does not help), the unroll fails with error X3511, "unable to unroll loop", and the
//! whole program with it, while Vulkan accepts the same source.
use super::*;
use wgpu::naga;

/// The world programs that compile the shared vertex path (skinning, deforms). The stage
/// table programs rewrite the stage program's bindings and fragments, not this path.
fn programs() -> Vec<(&'static str, String)> {
    vec![
        ("stage", STAGE_SHADER.to_owned()),
        ("real-time stage", world_sun_shader().to_owned()),
        ("entity light", entity_light_shader()),
        ("fog", crate::fog_volumes::SHADER.to_owned()),
    ]
}

/// Whether `pointer` reaches a vector or matrix component through a runtime index.
fn dynamic_component(
    module: &naga::Module,
    info: &naga::valid::FunctionInfo,
    function: &naga::Function,
    mut pointer: naga::Handle<naga::Expression>,
) -> bool {
    loop {
        match function.expressions[pointer] {
            naga::Expression::Access { base, .. } => {
                let pointee = match *info[base].ty.inner_with(&module.types) {
                    naga::TypeInner::Pointer { base, .. } => &module.types[base].inner,
                    ref other => other,
                };
                if matches!(
                    pointee,
                    naga::TypeInner::Vector { .. }
                        | naga::TypeInner::Matrix { .. }
                        | naga::TypeInner::ValuePointer { size: Some(_), .. }
                ) {
                    return true;
                }
                pointer = base;
            }
            naga::Expression::AccessIndex { base, .. } => pointer = base,
            _ => return false,
        }
    }
}

/// Whether a loop body opens like a `for` loop against a literal bound
/// (`if counter < 3u {} else { break; }`), the shape whose trip count FXC can evaluate.
fn constant_bound(function: &naga::Function, body: &naga::Block) -> bool {
    for statement in body.iter() {
        match statement {
            naga::Statement::Emit(_) => {}
            naga::Statement::If {
                condition, reject, ..
            } if matches!(reject.first(), Some(naga::Statement::Break)) => {
                return matches!(
                    function.expressions[*condition],
                    naga::Expression::Binary { right, .. }
                        if matches!(function.expressions[right], naga::Expression::Literal(_))
                );
            }
            _ => return false,
        }
    }
    false
}

/// Functions with a runtime-indexed component store inside a runtime-bounded loop nest.
fn unrollable_stores(
    module: &naga::Module,
    info: &naga::valid::FunctionInfo,
    function: &naga::Function,
    block: &naga::Block,
    runtime_loop: bool,
    found: &mut Vec<String>,
) {
    for statement in block.iter() {
        let mut visit = |inner: &naga::Block, runtime_loop: bool| {
            unrollable_stores(module, info, function, inner, runtime_loop, found)
        };
        match statement {
            naga::Statement::Store { pointer, .. } => {
                if runtime_loop && dynamic_component(module, info, function, *pointer) {
                    found.push(function.name.clone().unwrap_or_default());
                }
            }
            naga::Statement::Block(inner) => visit(inner, runtime_loop),
            naga::Statement::If { accept, reject, .. } => {
                visit(accept, runtime_loop);
                visit(reject, runtime_loop);
            }
            naga::Statement::Switch { cases, .. } => {
                for case in cases {
                    visit(&case.body, runtime_loop);
                }
            }
            naga::Statement::Loop {
                body, continuing, ..
            } => {
                let runtime_loop = runtime_loop || !constant_bound(function, body);
                visit(body, runtime_loop);
                visit(continuing, runtime_loop);
            }
            _ => {}
        }
    }
}

/// Every function of `source` whose loops FXC would fail to unroll.
fn fxc_unroll_failures(name: &str, source: &str) -> Vec<String> {
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|error| panic!("{name}: {}", error.emit_to_string(source)));
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|error| panic!("{name}: {}", error.emit_to_string(source)));
    let mut found = Vec::new();
    for (handle, function) in module.functions.iter() {
        let info = &info[handle];
        unrollable_stores(&module, info, function, &function.body, false, &mut found);
    }
    for (index, entry) in module.entry_points.iter().enumerate() {
        let (info, function) = (info.get_entry_point(index), &entry.function);
        unrollable_stores(&module, info, function, &function.body, false, &mut found);
    }
    found
}

#[test]
fn world_programs_avoid_fxc_unroll_failures() {
    for (name, source) in programs() {
        let found = fxc_unroll_failures(name, &source);
        assert!(
            found.is_empty(),
            "{name}: FXC cannot unroll the loops around a runtime-indexed component \
             store in {found:?}"
        );
    }
}

/// The shapes FXC was observed to accept and reject (`fxc /T vs_5_1`, naga-style loops).
#[test]
fn fxc_unroll_failures_are_detected() {
    let source = "
        fn runtime_bound(count: u32) -> vec3<f32> {
            var sum = vec3(0.0);
            for (var axis = 0u; axis < count; axis++) { sum[axis] += 1.0; }
            return sum;
        }
        fn constant_inside_runtime(count: u32) -> vec3<f32> {
            var sum = vec3(0.0);
            for (var i = 0u; i < count; i++) {
                for (var axis = 0u; axis < 3u; axis++) { sum[axis] += 1.0; }
            }
            return sum;
        }
        fn constant_bound(count: u32) -> vec3<f32> {
            var sum = vec3(0.0);
            for (var axis = 0u; axis < 3u; axis++) { sum[axis] += f32(count); }
            return sum;
        }
        fn whole_vector(count: u32) -> vec3<f32> {
            var sum = vec3(0.0);
            for (var i = 0u; i < count; i++) { sum += vec3(1.0); }
            return sum;
        }
        fn outside_loop(axis: u32) -> vec3<f32> {
            var sum = vec3(0.0);
            sum[axis] = 1.0;
            return sum;
        }
        @compute @workgroup_size(1) fn main() {
            _ = runtime_bound(3u) + constant_inside_runtime(3u) + constant_bound(3u)
                + whole_vector(3u) + outside_loop(1u);
        }
    ";
    assert_eq!(
        fxc_unroll_failures("sample", source),
        ["runtime_bound", "constant_inside_runtime"]
    );
}
