//! Build settings of the dedicated server: the build's version, commit and
//! commit time (`scripts/build_version.rs`, shared with the client), and on
//! Windows the program's icon and version strings. `sjk-server.exe` carries
//! SJK's icon (`assets/branding/sjk.ico`, which Explorer and shortcuts show)
//! and names itself "SJK dedicated server" in its version strings, which
//! Task Manager shows. Other platforms have no executable icons.

#[path = "../../scripts/build_version.rs"]
mod build_version;

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
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon(ICON)
        .set("ProductName", "SJK")
        .set("FileDescription", "SJK dedicated server")
        .set("ProductVersion", &version)
        .set("OriginalFilename", "sjk-server.exe");
    // A missing resource compiler costs the icon, not the build.
    if let Err(error) = resource.compile() {
        println!("cargo:warning=sjk-server is built without its icon: {error}");
    }
}
