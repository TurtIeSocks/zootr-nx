// No console window beside the app on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Arguments run the CLI; none open the window.
fn main() -> anyhow::Result<()> {
    if std::env::args_os().len() > 1 {
        attach_console();
        return zootr::cli::run();
    }
    zootr_app::run()?;
    Ok(())
}

/// Without a console of its own, the CLI prints to the one it was run from.
#[cfg(all(windows, not(debug_assertions)))]
fn attach_console() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(pid: u32) -> i32;
    }
    const PARENT: u32 = u32::MAX;
    unsafe { AttachConsole(PARENT) };
}

#[cfg(not(all(windows, not(debug_assertions))))]
fn attach_console() {}
