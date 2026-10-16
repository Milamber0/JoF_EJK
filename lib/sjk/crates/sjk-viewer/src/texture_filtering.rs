//! Optional world/model filtering, sampled once into the persistent graphics context.
use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry};

#[path = "world_texture_mips.rs"]
pub(crate) mod mips;

/// The six GL_TextureMode choices, retained without per-frame string parsing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Policy {
    mode: u8,
    anisotropy: u16,
}

const MODES: [&str; 6] = [
    "GL_LINEAR",
    "GL_NEAREST",
    "GL_NEAREST_MIPMAP_NEAREST",
    "GL_LINEAR_MIPMAP_NEAREST",
    "GL_NEAREST_MIPMAP_LINEAR",
    "GL_LINEAR_MIPMAP_LINEAR",
];

/// Register functioning sampler controls; changing a value explicitly requests a restart.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), CvarError> {
    for definition in [
        CvarDefinition::new(
            "r_ext_texture_filter_anisotropic",
            1_i64,
            CvarFlags::ARCHIVE,
            "Enable world/model anisotropy; vid_restart applies it",
        ),
        CvarDefinition::new(
            "r_ext_max_anisotropy",
            16_i64,
            CvarFlags::ARCHIVE,
            "Anisotropy level (1..16); vid_restart applies it",
        ),
        CvarDefinition::new(
            "r_textureMode",
            "GL_LINEAR_MIPMAP_LINEAR",
            CvarFlags::ARCHIVE,
            "GL texture filter; vid_restart applies it",
        ),
    ] {
        cvars.register(definition)?;
    }
    for name in [
        "r_ext_texture_filter_anisotropic",
        "r_ext_max_anisotropy",
        "r_texturemode",
    ] {
        cvars.on_change(name, move |_| {
            crate::log::progress(format_args!(
                "{name} changed: {}",
                crate::graphics_reload::APPLY
            ));
            crate::graphics_reload::notice();
        })?;
    }
    Ok(())
}

impl Policy {
    /// Clamp to WGPU's exposed native capability (1 or the portable 16x bound).
    pub(crate) fn resolve(mode: &str, enabled: bool, level: i64, maximum: u16) -> Option<Self> {
        let mode = MODES
            .iter()
            .position(|name| name.eq_ignore_ascii_case(mode))? as u8;
        // Vulkan reference rejects anisotropy for nearest mag/mip filtering.
        let anisotropy = if enabled && mode == 5 {
            level.clamp(1, i64::from(maximum.clamp(1, 16))) as u16
        } else {
            1
        };
        Some(Self {
            mode,
            anisotropy: if anisotropy == 1 { 0 } else { anisotropy },
        })
    }

    /// What [`Self::sample`] would read now, without its report.
    pub(crate) fn read(console: Option<&crate::console::ViewerConsole>, maximum: u16) -> Self {
        let Some(console) = console else {
            return Self::resolve("GL_LINEAR_MIPMAP_LINEAR", true, 16, maximum).unwrap();
        };
        Self::resolve(
            console
                .text_value("r_texturemode")
                .unwrap_or("GL_LINEAR_MIPMAP_LINEAR"),
            console
                .integer_cvar("r_ext_texture_filter_anisotropic")
                .unwrap_or(1)
                != 0,
            console.integer_cvar("r_ext_max_anisotropy").unwrap_or(16),
            maximum,
        )
        .unwrap_or_default()
    }

    /// Read once at context construction. Invalid mode names are reported before falling back.
    pub(crate) fn sample(console: Option<&crate::console::ViewerConsole>, maximum: u16) -> Self {
        let Some(console) = console else {
            return Self::resolve("GL_LINEAR_MIPMAP_LINEAR", true, 16, maximum).unwrap();
        };
        let mode = console
            .text_value("r_texturemode")
            .unwrap_or("GL_LINEAR_MIPMAP_LINEAR");
        let result = Self::resolve(
            mode,
            console
                .integer_cvar("r_ext_texture_filter_anisotropic")
                .unwrap_or(1)
                != 0,
            console.integer_cvar("r_ext_max_anisotropy").unwrap_or(16),
            maximum,
        );
        if result.is_none() {
            crate::log::progress(format_args!(
                "Invalid r_texturemode {mode}; using GL_LINEAR"
            ));
        }
        let policy = result.unwrap_or_default();
        if policy.mode != 5
            && console
                .integer_cvar("r_ext_texture_filter_anisotropic")
                .unwrap_or(1)
                != 0
        {
            crate::log::progress(format_args!(
                "anisotropy requires r_textureMode GL_LINEAR_MIPMAP_LINEAR; vid_restart after setting"
            ));
        }
        crate::log::progress(format_args!(
            "world/model filtering: mode={} anisotropy={}x portable-cap={}x; vid_restart to change",
            MODES[usize::from(policy.mode)],
            policy.anisotropy.max(1),
            maximum
        ));
        policy
    }

    /// Mip chains are optional; GL_LINEAR retains the original single-level base experience.
    pub(crate) fn mipmapped(self) -> bool {
        self.mode >= 2
    }

    /// Build the repeat/clamp sampler shared by world and rigid/skinned model stages.
    pub(crate) fn descriptor(self, clamp: bool) -> wgpu::SamplerDescriptor<'static> {
        let address = if clamp {
            wgpu::AddressMode::ClampToEdge
        } else {
            wgpu::AddressMode::Repeat
        };
        let nearest = matches!(self.mode, 1 | 2 | 4);
        let filter = if nearest {
            wgpu::FilterMode::Nearest
        } else {
            wgpu::FilterMode::Linear
        };
        wgpu::SamplerDescriptor {
            label: Some("SJK Q3 stage sampler"),
            address_mode_u: address,
            address_mode_v: address,
            address_mode_w: address,
            mag_filter: filter,
            min_filter: filter,
            mipmap_filter: if matches!(self.mode, 2 | 3) {
                wgpu::MipmapFilterMode::Nearest
            } else {
                wgpu::MipmapFilterMode::Linear
            },
            anisotropy_clamp: self.anisotropy.max(1),
            ..Default::default()
        }
    }
}
