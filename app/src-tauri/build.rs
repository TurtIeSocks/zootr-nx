fn main() {
    // Tauri puts the Windows manifest (Common Controls v6) in the app alone.
    // A test binary without it can't start: STATUS_ENTRYPOINT_NOT_FOUND. So
    // the linker embeds it in every binary instead, tests included.
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    let attributes = tauri_build::Attributes::new().windows_attributes(windows);
    tauri_build::try_build(attributes).expect("tauri build");
    apworld();
    commit();
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

/// Bakes in `ZNX_COMMIT`: the short commit this was built from, with `*`
/// when tracked files had changes, or empty without git (a source tarball).
/// It reruns when HEAD, its branch or the index moves (`logs/HEAD` catches
/// a commit to a branch whose ref isn't a file yet), so an edit nobody
/// staged since the last build doesn't add the `*`. `ZNX_RELEASE`, which
/// the release workflow sets for a tagged build, marks a release.
fn commit() {
    println!("cargo:rerun-if-env-changed=ZNX_RELEASE");
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git").args(args).output().ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    };
    let branch = git(&["symbolic-ref", "-q", "HEAD"]);
    let watched = ["HEAD", "logs/HEAD", "index", "packed-refs"]
        .into_iter()
        .map(str::to_owned);
    for name in watched.chain(branch) {
        if let Some(path) = git(&["rev-parse", "--git-path", &name])
            && std::path::Path::new(&path).exists()
        {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    let commit = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_default();
    let dirty = !commit.is_empty()
        && git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty());
    println!(
        "cargo:rustc-env=ZNX_COMMIT={commit}{}",
        if dirty { "*" } else { "" }
    );
}
