fn main() {
    // Tauri puts the Windows manifest (Common Controls v6) in the app alone.
    // A test binary without it can't start: STATUS_ENTRYPOINT_NOT_FOUND. So
    // the linker embeds it in every binary instead, tests included.
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    let attributes = tauri_build::Attributes::new().windows_attributes(windows);
    tauri_build::try_build(attributes).expect("tauri build");
    apworld();
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let dir = std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo");
        let manifest = std::path::Path::new(&dir).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}

/// Embeds the znx.apworld named by `ZNX_APWORLD`, which the release builds
/// from this same source with Archipelago's packager, so hosts get the copy
/// matching their app. Other builds embed an empty file.
fn apworld() {
    println!("cargo:rerun-if-env-changed=ZNX_APWORLD");
    let out =
        std::path::Path::new(&std::env::var("OUT_DIR").expect("set by cargo")).join("znx.apworld");
    match std::env::var_os("ZNX_APWORLD") {
        Some(from) => {
            println!("cargo:rerun-if-changed={}", from.to_string_lossy());
            std::fs::copy(&from, &out).expect("ZNX_APWORLD names a readable file");
        }
        None => std::fs::write(&out, []).expect("write OUT_DIR"),
    }
}
