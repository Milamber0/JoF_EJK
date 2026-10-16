//! Card faces (`docs/cards.md`, "Faces"): what a revealed card shows, read from the SJK
//! packs the hub serves (`sjk_cards`, [`crate::sjk_packs`]). Each card is a file
//! `cards/<id>.card` (JSON: its name, set, number, kind, rarity, lore, artist and the game
//! path of its picture) beside its picture, a JPEG or PNG framed for the card's art
//! window ([`ART_ASPECT`]). The hub keeps which card each copy is; the pack says what it
//! looks like, so a client draws a card it has the face of and shows "Unknown card"
//! (its id) for one it has not.
//!
//! The faces are read whenever the packs are mounted ([`install`]); their pictures are
//! decoded on a worker thread, squeezed into a square mip chain the UI renderer uploads
//! (`ui_renderer/card_art.rs`) and drawn back into the 6:5 window, which undoes the
//! squeeze. Until a picture is decoded the window stays dark.

use super::Rarity;
use crate::menu::emblem::MipChain;
use serde::Deserialize;
use sjk_ui::TextureId;
use sjk_vfs::VirtualFileSystem;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

/// Where the card files are in a pack, and their extension.
pub(crate) const FOLDER: &str = "cards";
pub(crate) const EXTENSION: &str = ".card";
/// The art window's width against its height: 6:5 (pictures are 1020 x 850).
pub(crate) const ART_ASPECT: f32 = 1.2;
/// The square texture a picture is squeezed into.
const TEXTURE_EDGE: u32 = 1_024;
/// Most faces the client draws pictures for; one texture each.
pub(crate) const MAX_FACES: usize = 512;
/// Card pictures' texture ids: this, plus the face's position in [`Faces::list`].
const TEXTURE_BASE: u32 = 0x6100_0000;
/// The longest name, set name and artist, and lore a face may have (as the hub's pack
/// build checks).
const NAME_MAX: usize = 40;
const LORE_MAX: usize = 400;

/// What a card is about.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    Community,
    Place,
    Personality,
    Game,
}

impl Kind {
    fn from_id(id: &str) -> Option<Self> {
        match id {
            "community" => Some(Self::Community),
            "place" => Some(Self::Place),
            "personality" => Some(Self::Personality),
            "game" => Some(Self::Game),
            _ => None,
        }
    }

    /// What players read ("Community").
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Community => "Community",
            Self::Place => "Place",
            Self::Personality => "Personality",
            Self::Game => "Jedi Academy",
        }
    }
}

/// One card's face.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Face {
    /// The card's id ("sol_lore_001"), the hub's.
    pub(crate) id: String,
    pub(crate) name: String,
    /// The set's id ("sol_lore") and the name players read ("Sol lore").
    pub(crate) set: String,
    pub(crate) set_name: String,
    /// Its place in its set, from 1.
    pub(crate) number: u16,
    pub(crate) kind: Kind,
    pub(crate) rarity: &'static Rarity,
    pub(crate) lore: String,
    pub(crate) artist: String,
    /// The game path of its picture in the pack.
    pub(crate) art: String,
}

/// A card file as written.
#[derive(Deserialize)]
struct File {
    name: String,
    set: String,
    set_name: String,
    number: u16,
    kind: String,
    rarity: String,
    #[serde(default)]
    lore: String,
    art: String,
    #[serde(default)]
    artist: String,
}

/// The card file `text` of card `id`, read.
fn parse(id: &str, text: &str) -> Result<Face, String> {
    let file: File = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let within = |what: &str, value: &str, max: usize| {
        if value.trim().is_empty() || value.chars().count() > max {
            Err(format!("{what} must be 1 to {max} characters"))
        } else {
            Ok(())
        }
    };
    within("name", &file.name, NAME_MAX)?;
    within("set_name", &file.set_name, NAME_MAX)?;
    if file.lore.chars().count() > LORE_MAX {
        return Err(format!("lore is longer than {LORE_MAX} characters"));
    }
    if !(1..=999).contains(&file.number) {
        return Err("number must be 1 to 999".to_owned());
    }
    let kind = Kind::from_id(&file.kind).ok_or_else(|| format!("unknown kind {}", file.kind))?;
    let rarity =
        Rarity::from_id(&file.rarity).ok_or_else(|| format!("unknown rarity {}", file.rarity))?;
    Ok(Face {
        id: id.to_owned(),
        name: file.name,
        set: file.set,
        set_name: file.set_name,
        number: file.number,
        kind,
        rarity,
        lore: file.lore,
        artist: if file.artist.trim().is_empty() {
            "Sol".to_owned()
        } else {
            file.artist
        },
        art: file.art,
    })
}

/// Every card face in `vfs`, by set and number; a file that cannot be read is named in
/// the log and left out.
pub(crate) fn load(vfs: &VirtualFileSystem) -> Vec<Face> {
    let mut faces = Vec::new();
    for listed in vfs.list_files(FOLDER, EXTENSION) {
        let path = format!("{FOLDER}/{listed}");
        let id = listed.trim_end_matches(EXTENSION);
        let read = vfs
            .read(&path)
            .map_err(|error| error.to_string())
            .and_then(|asset| asset.ok_or_else(|| "missing".to_owned()))
            .and_then(|asset| String::from_utf8(asset.bytes).map_err(|_| "not UTF-8".to_owned()))
            .and_then(|text| parse(id, &text));
        match read {
            Ok(face) => faces.push(face),
            Err(why) => crate::log::progress(format_args!("cards: {path}: {why}")),
        }
    }
    faces.sort_by(|a, b| (&a.set, a.number, &a.id).cmp(&(&b.set, b.number, &b.id)));
    faces.dedup_by(|a, b| a.id == b.id);
    faces.truncate(MAX_FACES);
    faces
}

/// The faces of the last mount and their pictures, as they decode.
pub(crate) struct Faces {
    pub(crate) list: Vec<Face>,
    /// Each face's picture, once decoded (`None` inside when it could not be).
    art: Vec<OnceLock<Option<MipChain>>>,
    /// Counts mounts; the renderer uploads the pictures again when it changes.
    pub(crate) generation: u64,
}

impl Faces {
    fn empty() -> Self {
        Self {
            list: Vec::new(),
            art: Vec::new(),
            generation: 0,
        }
    }

    /// The face of card `id`, with its position in [`Self::list`].
    pub(crate) fn find(&self, id: &str) -> Option<(usize, &Face)> {
        self.list.iter().enumerate().find(|(_, face)| face.id == id)
    }

    /// How many faces of set `set` there are.
    pub(crate) fn in_set(&self, set: &str) -> usize {
        self.list.iter().filter(|face| face.set == set).count()
    }

    /// Whether some face has rarity `rarity`: sealed copies of it can be revealed.
    pub(crate) fn has_rarity(&self, rarity: &Rarity) -> bool {
        self.list.iter().any(|face| face.rarity.id == rarity.id)
    }

    /// Faces `list` without pictures, for tests (not the current ones).
    #[cfg(test)]
    pub(crate) fn of_list(list: Vec<Face>) -> Self {
        Self {
            art: list.iter().map(|_| OnceLock::new()).collect(),
            list,
            generation: 1,
        }
    }

    /// The decoded picture of face `index`, once it is.
    pub(crate) fn art(&self, index: usize) -> Option<&MipChain> {
        self.art.get(index)?.get()?.as_ref()
    }
}

static FACES: Mutex<Option<Arc<Faces>>> = Mutex::new(None);

fn lock() -> MutexGuard<'static, Option<Arc<Faces>>> {
    FACES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The faces of the last mount (none before one).
pub(crate) fn current() -> Arc<Faces> {
    lock().clone().unwrap_or_else(|| Arc::new(Faces::empty()))
}

/// The texture of face `index`'s picture.
pub(crate) fn texture(index: usize) -> TextureId {
    TextureId(TEXTURE_BASE + index.min(MAX_FACES - 1) as u32)
}

/// The face position a texture id stands for, if it is a card picture's.
pub(crate) fn index_of(texture: TextureId) -> Option<usize> {
    let index = texture.0.checked_sub(TEXTURE_BASE)? as usize;
    (index < MAX_FACES).then_some(index)
}

/// Read the faces of the packs `vfs` (just mounted, under `generation`), make them the
/// current ones, and decode their pictures on a worker thread.
pub(crate) fn install(vfs: &VirtualFileSystem, generation: u64) {
    let list = load(vfs);
    let pictures: Vec<Option<Vec<u8>>> = list
        .iter()
        .map(|face| vfs.read(&face.art).ok().flatten().map(|asset| asset.bytes))
        .collect();
    let faces = Arc::new(Faces {
        art: list.iter().map(|_| OnceLock::new()).collect(),
        list,
        generation,
    });
    if !faces.list.is_empty() {
        crate::log::progress(format_args!(
            "SJK packs: card faces: {}",
            faces
                .list
                .iter()
                .map(|face| face.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    *lock() = Some(Arc::clone(&faces));
    if faces.list.is_empty() {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("sjk-card-art".into())
        .spawn(move || {
            for (index, bytes) in pictures.into_iter().enumerate() {
                let face = &faces.list[index];
                let chain = match bytes {
                    Some(bytes) => match decode(&bytes) {
                        Ok(chain) => Some(chain),
                        Err(why) => {
                            crate::log::progress(format_args!("cards: {}: {why}", face.art));
                            None
                        }
                    },
                    None => {
                        crate::log::progress(format_args!("cards: {}: missing", face.art));
                        None
                    }
                };
                let _ = faces.art[index].set(chain);
            }
        });
    if let Err(error) = spawned {
        crate::log::progress(format_args!(
            "warning: card pictures not loaded, drawing without them: {error}"
        ));
    }
}

/// A picture's bytes as the square mip chain the renderer uploads: cut to the window's
/// 6:5 about its middle when it is not 6:5, then squeezed to [`TEXTURE_EDGE`] square.
pub(crate) fn decode(bytes: &[u8]) -> Result<MipChain, String> {
    let image = image::load_from_memory(bytes)
        .map_err(|error| error.to_string())?
        .into_rgba8();
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return Err("the picture is empty".to_owned());
    }
    let wide = width as f32 / height as f32;
    let (cut_w, cut_h) = if wide > ART_ASPECT {
        ((height as f32 * ART_ASPECT).round() as u32, height)
    } else {
        (width, (width as f32 / ART_ASPECT).round() as u32)
    };
    let cut = image::imageops::crop_imm(
        &image,
        (width - cut_w.min(width)) / 2,
        (height - cut_h.min(height)) / 2,
        cut_w.min(width),
        cut_h.min(height),
    )
    .to_image();
    let square = image::imageops::resize(
        &cut,
        TEXTURE_EDGE,
        TEXTURE_EDGE,
        image::imageops::FilterType::Triangle,
    );
    Ok(MipChain::new(square))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"{
  "name": "Toxiee and Sol on JoF Memorial",
  "set": "sol_lore",
  "set_name": "Sol lore",
  "number": 1,
  "kind": "community",
  "rarity": "legendary",
  "lore": "Tox was simply chilling.",
  "art": "cards/sol_lore_001.jpg",
  "artist": "Sol"
}
"#;

    #[test]
    fn a_card_file_reads_and_bad_ones_are_refused() {
        let face = parse("sol_lore_001", FILE).unwrap();
        assert_eq!(face.name, "Toxiee and Sol on JoF Memorial");
        assert_eq!((face.set.as_str(), face.number), ("sol_lore", 1));
        assert_eq!(face.kind, Kind::Community);
        assert_eq!(face.rarity.id, "legendary");
        assert_eq!(face.art, "cards/sol_lore_001.jpg");
        for (from, to) in [
            ("\"legendary\"", "\"mythic\""),
            ("\"community\"", "\"memes\""),
            ("\"number\": 1", "\"number\": 0"),
            ("Toxiee and Sol on JoF Memorial", ""),
        ] {
            assert!(parse("x", &FILE.replace(from, to)).is_err(), "{to}");
        }
        // A field the client does not know yet is read past.
        let later = FILE.replace("\"artist\"", "\"frame\": \"gold\", \"artist\"");
        assert!(parse("x", &later).is_ok());
    }

    #[test]
    fn a_picture_is_squeezed_into_a_square_chain() {
        let mut png = Vec::new();
        let picture = image::RgbaImage::from_pixel(1_020, 850, image::Rgba([10, 20, 30, 255]));
        image::DynamicImage::ImageRgba8(picture)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let chain = decode(&png).unwrap();
        assert_eq!(chain.size, TEXTURE_EDGE);
        assert_eq!(chain.levels.len(), 11, "1024 down to 1");
        assert!(decode(b"not a picture").is_err());
    }

    #[test]
    fn texture_ids_round_trip() {
        assert_eq!(index_of(texture(3)), Some(3));
        assert_eq!(index_of(TextureId(TEXTURE_BASE - 1)), None);
        assert_eq!(index_of(TextureId(TEXTURE_BASE + MAX_FACES as u32)), None);
    }
}
