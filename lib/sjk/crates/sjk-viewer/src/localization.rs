//! Retail string-table localization kept outside rendering/UI layout code.

use sjk_vfs::VirtualFileSystem;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct Localization {
    pub(crate) strings: HashMap<String, String>,
}

impl Localization {
    pub(crate) fn load(vfs: &VirtualFileSystem) -> Self {
        let mut localization = Self::default();
        for path in [
            "strings/english/mp_ingame.str",
            "strings/english/mp_svgame.str",
        ] {
            let Some(asset) = vfs.read(path).ok().flatten() else {
                continue;
            };
            let text = String::from_utf8_lossy(&asset.bytes);
            sjk_client::string_table::parse_into(&mut localization.strings, &text);
        }
        localization
    }

    pub(crate) fn translate(&self, text: &str) -> String {
        let mut translated = String::with_capacity(text.len());
        let mut remaining = text;
        while let Some(position) = remaining.find("@@@") {
            translated.push_str(&remaining[..position]);
            let token = &remaining[position + 3..];
            let length = token
                .bytes()
                .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                .count();
            if length == 0 {
                translated.push_str("@@@");
                remaining = token;
                continue;
            }
            let key = &token[..length];
            translated.push_str(self.strings.get(key).map_or(key, String::as_str));
            remaining = &token[length..];
        }
        translated.push_str(remaining);
        translated
    }
}
