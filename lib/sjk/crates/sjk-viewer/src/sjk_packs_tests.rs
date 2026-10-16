use super::*;
use crate::blade_skin_file::tests::SAMPLE;
use std::io::Write;

/// Write a PK3 named `name` into `directory` holding `files`.
fn write_pack(directory: &Path, name: &str, files: &[(&str, &[u8])]) {
    let file = std::fs::File::create(directory.join(name)).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    for (path, bytes) in files {
        zip.start_file(*path, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn a_pack_arriving_at_runtime_is_mounted_and_its_skins_loaded() {
    let directory = tempfile::tempdir().unwrap();
    // Nothing cached yet: no skins, under a new generation all the same.
    let before = generation();
    let empty = mount(directory.path());
    assert!(empty.ids().next().is_none());
    assert!(empty.generation() > before);
    // The service writes a pack: mounting again loads its blade skin, its sounds come
    // from it, and frames see the new generation.
    write_pack(
        directory.path(),
        "sjk_skins.pk3",
        &[
            ("skins/blades/saber_sun.bladeskin", SAMPLE.as_bytes()),
            ("sound/test/blade/hum.wav", b"RIFF"),
        ],
    );
    // Partial downloads and other files are not packs.
    std::fs::write(directory.path().join(".sjk_skins.pk3.part"), b"x").unwrap();
    std::fs::write(directory.path().join("notes.txt"), b"x").unwrap();
    let loaded = mount(directory.path());
    assert_eq!(loaded.ids().collect::<Vec<_>>(), ["saber_sun"]);
    assert!(loaded.generation() > empty.generation());
    assert!(generation() >= loaded.generation());
    assert!(
        loaded
            .packs()
            .read("sound/test/blade/hum.wav")
            .unwrap()
            .is_some()
    );
    assert!(skins().generation() >= loaded.generation());
}

#[test]
fn a_broken_pack_is_skipped_and_the_others_still_mount() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("aaa.pk3"), b"not a zip").unwrap();
    write_pack(
        directory.path(),
        "sjk_skins.pk3",
        &[("skins/blades/saber_sun.bladeskin", SAMPLE.as_bytes())],
    );
    let packs = pack_file_system(directory.path());
    assert_eq!(packs.mounts().count(), 1);
    assert!(
        packs
            .read("skins/blades/saber_sun.bladeskin")
            .unwrap()
            .is_some()
    );
}

/// The SJK hub's packs (its `assets/dist`, named by `SJK_TEST_PACKS`) hold a blade-skin
/// file for every blade skin of this client's catalogue that this client reads, and every
/// sound each names. The hub's art is not in this repository, so this runs by hand.
#[test]
#[ignore = "needs the SJK hub's packs, named by SJK_TEST_PACKS"]
fn the_hubs_packs_hold_every_blade_skin_of_the_catalogue() {
    let directory = std::env::var_os("SJK_TEST_PACKS").expect("SJK_TEST_PACKS");
    let packs = pack_file_system(Path::new(&directory));
    let skins = LoadedSkins::load(packs.clone(), 1);
    let expected: Vec<&str> = crate::unlockables::blade_skins()
        .map(|skin| skin.id)
        .collect();
    let mut loaded: Vec<&str> = skins.ids().collect();
    loaded.sort_unstable();
    let mut wanted = expected.clone();
    wanted.sort_unstable();
    assert_eq!(
        loaded, wanted,
        "every blade skin, read by the strict parser"
    );
    for set in skins.sound_sets() {
        for path in [set.on, set.off, set.hum].into_iter().chain(set.swings) {
            let sound = packs.read(path).unwrap();
            assert!(
                sound.is_some_and(|sound| sound.bytes.starts_with(b"RIFF")),
                "{path} is not a WAV in the packs"
            );
        }
    }
}
