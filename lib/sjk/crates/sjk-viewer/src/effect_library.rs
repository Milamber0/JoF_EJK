//! Map-lifetime cache for parsed EFX graphs and their shader names.

use sjk_effect::{EffectDefinition, load_effect};
use sjk_vfs::VirtualFileSystem;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Parsed effect definitions and normalized shader-name storage.
#[derive(Default)]
pub(crate) struct EffectLibrary {
    definitions: HashMap<String, Arc<EffectDefinition>>,
    shaders: HashMap<String, Arc<str>>,
    asset_names: HashMap<String, Arc<str>>,
    code_primitive: Option<Arc<EffectDefinition>>,
}

impl EffectLibrary {
    /// Check a canonical preloaded name without allocating or retrying missing assets.
    pub(crate) fn contains_definition(&self, name: &str) -> bool {
        self.definitions.contains_key(name)
    }

    /// The empty definition backing primitives built from code rather than
    /// from an EFX graph (`FX_AddLine` beams); it carries no physics.
    pub(crate) fn code_primitive_definition(&mut self) -> Arc<EffectDefinition> {
        Arc::clone(
            self.code_primitive
                .get_or_insert_with(|| Arc::new(EffectDefinition::default())),
        )
    }

    /// Gather shaders from all preloaded graphs, including nested effects.
    pub(crate) fn append_shader_paths(&self, output: &mut std::collections::BTreeSet<String>) {
        for definition in self.definitions.values() {
            for component in &definition.components {
                output.extend(
                    component
                        .shaders
                        .iter()
                        .map(|name| name.to_ascii_lowercase()),
                );
            }
        }
    }

    pub(crate) fn append_sound_paths(&self, output: &mut Vec<String>) {
        output.clear();
        for definition in self.definitions.values() {
            for component in &definition.components {
                output.extend(component.sounds.iter().cloned());
            }
        }
        output.sort_unstable();
        output.dedup();
    }

    pub(crate) fn definition(
        &mut self,
        vfs: &VirtualFileSystem,
        name: &str,
    ) -> Option<Arc<EffectDefinition>> {
        let normalized = name
            .trim()
            .trim_start_matches("effects/")
            .trim_end_matches(".efx");
        if let Some(definition) = self.definitions.get(normalized) {
            return Some(Arc::clone(definition));
        }
        let key = normalized.to_ascii_lowercase();
        if !self.definitions.contains_key(&key) {
            match load_effect(vfs, &key) {
                Ok(definition) => {
                    self.definitions.insert(key.clone(), Arc::new(definition));
                }
                Err(error) => {
                    eprintln!("could not load effect {name}: {error}");
                    return None;
                }
            }
        }
        let definition = self.definitions.get(&key).cloned()?;
        if normalized != key {
            self.definitions
                .insert(normalized.to_owned(), Arc::clone(&definition));
        }
        Some(definition)
    }

    pub(crate) fn shader(&mut self, name: &str) -> Arc<str> {
        if let Some(shader) = self.shaders.get(name) {
            return Arc::clone(shader);
        }
        let normalized = name.to_ascii_lowercase();
        if let Some(shader) = self.shaders.get(normalized.as_str()) {
            let shader = Arc::clone(shader);
            self.shaders.insert(name.to_owned(), Arc::clone(&shader));
            return shader;
        }
        let shader: Arc<str> = Arc::from(normalized.as_str());
        self.shaders.insert(normalized, Arc::clone(&shader));
        if name.as_bytes() != shader.as_bytes() {
            self.shaders.insert(name.to_owned(), Arc::clone(&shader));
        }
        shader
    }

    pub(crate) fn asset_name(&mut self, name: &str) -> Arc<str> {
        if let Some(value) = self.asset_names.get(name) {
            return Arc::clone(value);
        }
        let normalized = name.to_ascii_lowercase();
        let value: Arc<str> = Arc::from(normalized.as_str());
        self.asset_names.insert(normalized, Arc::clone(&value));
        value
    }

    pub(crate) fn preload<I, S>(&mut self, vfs: &VirtualFileSystem, names: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut pending = names
            .into_iter()
            .map(|name| name.as_ref().to_owned())
            .collect::<Vec<_>>();
        let mut visited = HashSet::new();
        while let Some(name) = pending.pop() {
            if !visited.insert(name.to_ascii_lowercase()) {
                continue;
            }
            let Some(definition) = self.definition(vfs, &name) else {
                continue;
            };
            for component in &definition.components {
                for shader in &component.shaders {
                    let _ = self.shader(shader);
                }
                pending.extend(component.effects.iter().cloned());
                pending.extend(component.impact_effects.iter().cloned());
                pending.extend(component.death_effects.iter().cloned());
                pending.extend(component.emit_effects.iter().cloned());
                for model in &component.models {
                    let _ = self.asset_name(model);
                }
            }
        }
    }
}
