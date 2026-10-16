//! `sjk-materialgen` command: see the library documentation and `--help`.

use sjk_materialgen::cli::{Command, USAGE, parse};
use sjk_materialgen::mount::client_loads;
use sjk_materialgen::package::NOTICE;
use sjk_materialgen::run::{Summary, describe_candidate, run};
use std::collections::BTreeMap;
use std::process::ExitCode;

fn main() -> ExitCode {
    let options = match parse(std::env::args().skip(1)) {
        Ok(Command::Help) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Ok(Command::Run(options)) => options,
        Err(error) => {
            eprintln!("sjk-materialgen: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!("game data: {}", options.game_data.display());
    // Loud on purpose: a pack built without the player's rules looked like a renderer bug.
    println!(
        "overrides: {}, {} rules",
        options.overrides_found.describe(),
        options.overrides.rules.len()
    );
    if matches!(
        options.overrides_found,
        sjk_materialgen::overrides_file::Found::Missing(_)
    ) {
        eprintln!(
            "sjk-materialgen: WARNING: no overrides file; pass --overrides FILE or put {} \
             in the client's generated folder (--no-overrides silences this)",
            sjk_materialgen::overrides_file::FILE_NAME
        );
    }
    let summary = match run(&options) {
        Ok(summary) => summary,
        Err(error) => {
            eprintln!("sjk-materialgen: {error}");
            return ExitCode::FAILURE;
        }
    };
    report(&options, &summary);
    if summary.failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn report(options: &sjk_materialgen::run::Options, summary: &Summary) {
    let selection = &summary.selection;
    for archive in &summary.excluded {
        println!("left out earlier generated pack: {}", archive.display());
    }
    println!(
        "maps: {} ({})",
        selection.maps.len(),
        abbreviated(&selection.maps)
    );
    let mut reasons: BTreeMap<&str, usize> = BTreeMap::new();
    for skipped in &selection.skipped {
        *reasons.entry(skipped.reason.describe()).or_default() += 1;
    }
    println!("skipped shaders/textures: {}", selection.skipped.len());
    for (reason, count) in &reasons {
        println!("  {count:>5}  {reason}");
    }
    if options.dry_run {
        println!(
            "would generate maps for {} textures:",
            selection.candidates.len()
        );
        for candidate in &selection.candidates {
            println!("  {}", describe_candidate(candidate));
        }
        for skipped in &selection.skipped {
            println!("  skip {:<44} {}", skipped.reason.describe(), skipped.name);
        }
        println!(
            "dry run: nothing written ({:.1} s)",
            summary.elapsed.as_secs_f64()
        );
        return;
    }
    if !summary.flat.is_empty() {
        println!("no surface detail, no maps: {}", abbreviated(&summary.flat));
    }
    println!(
        "emission maps: {} written for {} textures with signs of light (see the manifest)",
        summary.emission_written, summary.emission_considered
    );
    for (image, error) in &summary.failed {
        eprintln!("failed: {image}: {error}");
    }
    println!(
        "generated {} images for {} textures in {:.1} s: {} ({:.1} MiB)",
        summary.images,
        summary.generated,
        summary.elapsed.as_secs_f64(),
        options.out.display(),
        summary.bytes as f64 / (1024.0 * 1024.0)
    );
    let directory = options
        .out
        .parent()
        .map_or_else(|| ".".into(), |p| p.display().to_string());
    println!();
    if client_loads(&options.out, &options.game_data) {
        println!("SJK loads it from {directory} above every other pack and leaves out");
        println!("older generated packs in the game folders.");
    } else {
        println!("To try the maps, start SJK with");
        println!("  SJK_CONTENT={directory}");
        println!(
            "(cmd: set \"SJK_CONTENT={directory}\"; PowerShell: $env:SJK_CONTENT=\"{directory}\")."
        );
    }
    println!("Then: seta r_normalMapping 1; seta r_specularMapping 1; restart.");
    println!("Emission maps show with r_emissiveMaps 1 (the default).");
    println!();
    println!("{NOTICE}");
}

fn abbreviated(names: &[String]) -> String {
    if names.len() <= 8 {
        names.join(", ")
    } else {
        format!("{}, ... {} more", names[..8].join(", "), names.len() - 8)
    }
}
