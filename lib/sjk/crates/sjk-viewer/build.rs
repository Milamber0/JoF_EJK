//! Build settings of the client: the build's version, commit and commit time
//! (`scripts/build_version.rs`, shared with the dedicated server), and on
//! Windows the main thread's stack and the program's icon and version strings.
//!
//! Windows reserves 1 MiB for a program's main thread where Linux reserves
//! 8 MiB; the client overflows 1 MiB while loading a map, so Windows binaries
//! are linked with the Linux size. `sjk.exe` carries SJK's icon
//! (`assets/branding/sjk.ico`, which Explorer and shortcuts show) and names
//! itself "SJK" in its version strings, which Task Manager shows. Other
//! platforms have no executable icons.

#[path = "../../scripts/build_version.rs"]
mod build_version;

const MAIN_STACK_BYTES: u32 = 8 * 1024 * 1024;
/// SJK's icon set, relative to this crate.
const ICON: &str = "../../assets/branding/sjk.ico";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ICON}");
    println!("cargo:rerun-if-changed=../../scripts/build_version.rs");
    let version = build_version::emit();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
        Ok("msvc") => println!("cargo:rustc-link-arg-bins=/STACK:{MAIN_STACK_BYTES}"),
        Ok("gnu") => println!("cargo:rustc-link-arg-bins=-Wl,--stack,{MAIN_STACK_BYTES}"),
        _ => {}
    }
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon(ICON)
        .set("ProductName", "SJK")
        .set("FileDescription", "SJK")
        .set("ProductVersion", &version)
        .set("OriginalFilename", "sjk.exe");
    // A missing resource compiler costs the icon, not the build.
    if let Err(error) = resource.compile() {
        println!("cargo:warning=sjk is built without its icon: {error}");
    }
}
