//! A local Archipelago room for tests: a two-slot multiworld (Link and
//! Zelda) generated from a preset, hosted by `MultiServer.py` with a FIFO
//! on its stdin, so the server's cheat console can send items.
//!
//! The room runs in its own detached worktree of the Archipelago checkout,
//! whose `worlds/znx` points at the calling checkout's world: the
//! checkout's own link stays as other sessions left it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use super::zip;

/// The two slots every room holds, Link's first.
pub const SLOTS: [&str; 2] = ["Link", "Zelda"];

/// Archipelago's default port.
pub const PORT: u16 = 38281;

/// The seed rooms and bench runs generate with unless told otherwise.
pub const SEED: u64 = 4646;

/// What each slot starts with: the test seeds' items, by AP name.
const START: [&str; 5] = [
    "Kokiri Sword",
    "Deku Shield",
    "Progressive Slingshot",
    "Progressive Bomb Bag",
    "Progressive Ocarina",
];

/// Where the checkout is, when `--archipelago` isn't given.
pub const ENV: &str = "ZOOTR_ARCHIPELAGO";

/// The server says this once it takes connections.
const LISTENING: &str = "server listening on";

const PRESETS: &str = include_str!("../../../../ap/bench/presets.toml");

/// Each preset's options, by setting id.
pub fn presets() -> Result<BTreeMap<String, BTreeMap<String, toml::Value>>> {
    toml::from_str(PRESETS).context("ap/bench/presets.toml")
}

/// `name`'s options, as the slot YAML's lines under the game.
fn options(name: &str) -> Result<Vec<String>> {
    let presets = presets()?;
    let preset =
        (presets.get(name)).with_context(|| format!("no preset {name}: {}", names(&presets)))?;
    preset
        .iter()
        .map(|(id, v)| {
            let value = match v {
                toml::Value::Boolean(b) => b.to_string(),
                toml::Value::String(s) => s.clone(),
                _ => bail!("{name}.{id}: a toggle or a choice"),
            };
            Ok(format!("{}: {value}", id.replace('-', "_")))
        })
        .collect()
}

fn names(presets: &BTreeMap<String, BTreeMap<String, toml::Value>>) -> String {
    presets.keys().cloned().collect::<Vec<_>>().join(", ")
}

/// One slot's player file.
fn player_yaml(slot: &str, options: &[String]) -> String {
    let mut out = format!("name: {slot}\ngame: {}\n{}:\n", super::GAME, super::GAME);
    for line in options {
        out += &format!("  {line}\n");
    }
    out += "  start_inventory_from_pool:\n";
    for item in START {
        out += &format!("    {item}: 1\n");
    }
    out
}

/// The Archipelago checkout and the room's own worktree of it.
pub struct Archipelago {
    pub checkout: PathBuf,
    pub worktree: PathBuf,
}

impl Archipelago {
    /// `given`, else [`ENV`]. The worktree lives in `home`.
    pub fn find(given: Option<PathBuf>, home: &Path) -> Result<Self> {
        let checkout = given
            .or_else(|| std::env::var_os(ENV).map(PathBuf::from))
            .with_context(|| format!("where's the Archipelago checkout? --archipelago or {ENV}"))?;
        ensure!(
            checkout.join("Generate.py").exists(),
            "{} isn't an Archipelago checkout",
            checkout.display()
        );
        Ok(Self {
            checkout,
            worktree: home.join("archipelago"),
        })
    }

    fn python(&self) -> PathBuf {
        self.checkout.join(".venv/bin/python")
    }

    /// The worktree at the checkout's HEAD, its `worlds/znx` pointing at
    /// `world`.
    pub fn prepare(&self, world: &Path) -> Result<()> {
        let git = |args: &[&str]| -> Result<()> {
            let mut cmd = Command::new("git");
            cmd.arg("-C").arg(&self.checkout).args(args);
            run(&mut cmd)
        };
        let wt = self.worktree.to_string_lossy();
        match self.worktree.join(".git").exists() {
            false => git(&["worktree", "add", "-q", "--detach", &wt, "HEAD"])?,
            true => {
                let head = output(
                    Command::new("git")
                        .arg("-C")
                        .arg(&self.checkout)
                        .args(["rev-parse", "HEAD"]),
                )?;
                let mut cmd = Command::new("git");
                cmd.arg("-C")
                    .arg(&self.worktree)
                    .args(["checkout", "-q", "--detach", head.trim()]);
                run(&mut cmd)?;
            }
        }
        let host = self.checkout.join("host.yaml");
        if host.exists() && !self.worktree.join("host.yaml").exists() {
            std::fs::copy(&host, self.worktree.join("host.yaml"))?;
        }
        let link = self.worktree.join("worlds/znx");
        if link.symlink_metadata().is_ok() {
            std::fs::remove_file(&link).or_else(|_| std::fs::remove_dir_all(&link))?;
        }
        let world = world
            .canonicalize()
            .with_context(|| format!("{}", world.display()))?;
        symlink(&world, &link)
    }

    /// Generates both slots from `preset` into `out`, emptied first.
    pub fn generate(&self, preset: &str, seed: u64, out: &Path) -> Result<Generated> {
        let options = options(preset)?;
        let players = self.worktree.join("Players");
        std::fs::remove_dir_all(&players).ok();
        std::fs::create_dir_all(&players)?;
        for slot in SLOTS {
            let file = players.join(format!("{}.yaml", slot.to_lowercase()));
            std::fs::write(file, player_yaml(slot, &options))?;
        }
        std::fs::remove_dir_all(out).ok();
        std::fs::create_dir_all(out)?;
        let mut cmd = Command::new(self.python());
        cmd.current_dir(&self.worktree).arg("Generate.py");
        cmd.arg("--seed").arg(seed.to_string());
        cmd.arg("--player_files_path").arg(&players);
        cmd.arg("--outputpath").arg(out);
        run(&mut cmd).context("Generate.py")?;
        Generated::unpack(out)
    }
}

/// What a generation wrote: the server's file and each slot's.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Generated {
    pub multi: PathBuf,
    /// In [`SLOTS`] order.
    pub slots: Vec<PathBuf>,
}

impl Generated {
    /// Opens the zip Archipelago wrote in `out` beside it.
    fn unpack(out: &Path) -> Result<Self> {
        let zip = only(out, |n| n.starts_with("AP_") && n.ends_with(".zip"))?;
        for (name, data) in zip::files(&std::fs::read(&zip)?)? {
            let name = Path::new(&name)
                .file_name()
                .context("a zip entry with no name")?;
            std::fs::write(out.join(name), data)?;
        }
        Self::find(out)
    }

    fn find(out: &Path) -> Result<Self> {
        let multi = only(out, |n| n.ends_with(".archipelago"))?;
        let slots = (SLOTS.iter().enumerate())
            .map(|(i, s)| only(out, |n| n.ends_with(&format!("_P{}_{s}.apznx", i + 1))))
            .collect::<Result<_>>()?;
        Ok(Self { multi, slots })
    }
}

/// The one file in `dir` whose name `is`.
fn only(dir: &Path, is: impl Fn(&str) -> bool) -> Result<PathBuf> {
    let mut found = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let path = e?.path();
        if path.file_name().and_then(|n| n.to_str()).is_some_and(&is) {
            found.push(path);
        }
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        n => bail!("{n} matching files in {}, not 1", dir.display()),
    }
}

/// A room that's up, kept in `room.json` so any shell can reach it.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Room {
    /// The server's process group.
    pub pid: u32,
    pub port: u16,
    pub seed: u64,
    pub preset: String,
    pub generated: Generated,
    pub fifo: PathBuf,
    pub log: PathBuf,
}

/// The room's folder, beside the app's config.
pub fn home() -> Result<PathBuf> {
    Ok(crate::config::Config::path()?.with_file_name("ap-room"))
}

fn state(home: &Path) -> PathBuf {
    home.join("room.json")
}

impl Room {
    /// The room up in `home`, if one is.
    pub fn load(home: &Path) -> Result<Option<Self>> {
        match std::fs::read_to_string(state(home)) {
            Ok(text) => Ok(Some(serde_json::from_str(&text)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn save(&self, home: &Path) -> Result<()> {
        Ok(std::fs::write(
            state(home),
            serde_json::to_string_pretty(self)?,
        )?)
    }

    pub fn address(&self) -> String {
        format!("localhost:{}", self.port)
    }

    /// Generates from `preset` and hosts it. Refuses while a room is up.
    pub fn up(
        ap: &Archipelago,
        home: &Path,
        world: &Path,
        preset: &str,
        seed: u64,
        port: u16,
    ) -> Result<Self> {
        if let Some(room) = Self::load(home)? {
            ensure!(
                !room.alive(),
                "a room is up at {}: `ap room down` first",
                room.address()
            );
            std::fs::remove_file(state(home)).ok();
        }
        ap.prepare(world)?;
        let out = home.join("out");
        let generated = ap.generate(preset, seed, &out)?;
        let fifo = out.join("server.in");
        let log = out.join("server.log");
        run(Command::new("mkfifo").arg(&fifo))?;
        let pid = serve(ap, &generated.multi, port, &fifo, &log)?;
        let room = Self {
            pid,
            port,
            seed,
            preset: preset.to_owned(),
            generated,
            fifo,
            log,
        };
        if let Err(e) = room.wait_listening(Duration::from_secs(90)) {
            room.stop();
            return Err(e);
        }
        room.save(home)?;
        Ok(room)
    }

    fn wait_listening(&self, limit: Duration) -> Result<()> {
        let start = Instant::now();
        while start.elapsed() < limit {
            let log = std::fs::read_to_string(&self.log).unwrap_or_default();
            if log.contains(LISTENING) {
                return Ok(());
            }
            ensure!(self.alive(), "the server stopped:\n{}", tail(&log, 10));
            std::thread::sleep(Duration::from_millis(250));
        }
        bail!("the server didn't listen within {} s", limit.as_secs())
    }

    /// Whether the server is still running.
    pub fn alive(&self) -> bool {
        Command::new("kill")
            .args(["-0", &self.pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    /// Sends `item` to `slot` through the server's cheat console.
    pub fn send(&self, slot: &str, item: &str) -> Result<()> {
        use std::io::Write;
        ensure!(self.alive(), "the room's server isn't running");
        let mut fifo = std::fs::OpenOptions::new().append(true).open(&self.fifo)?;
        fifo.write_all(send_line(slot, item).as_bytes())?;
        Ok(())
    }

    /// The server log's last `n` lines, without its public address.
    pub fn log_tail(&self, n: usize) -> String {
        let log = std::fs::read_to_string(&self.log).unwrap_or_default();
        tail(&log, n)
    }

    fn stop(&self) {
        let group = format!("-{}", self.pid);
        Command::new("kill")
            .args(["-TERM", "--", &group])
            .stderr(Stdio::null())
            .status()
            .ok();
        std::fs::remove_file(&self.fifo).ok();
    }

    /// Stops the server and forgets the room.
    pub fn down(self, home: &Path) -> Result<()> {
        self.stop();
        std::fs::remove_file(state(home))?;
        Ok(())
    }
}

fn send_line(slot: &str, item: &str) -> String {
    format!("/send {slot} {item}\n")
}

/// The last `n` lines of `log`, leaving out the line naming the public
/// address the server found and Python's warnings.
fn tail(log: &str, n: usize) -> String {
    let noise = |l: &&str| {
        l.starts_with("Hosting game at")
            || l.contains("Warning:")
            || l.starts_with("  import ")
            || l.starts_with("  warnings.warn")
    };
    let lines: Vec<&str> = log.lines().filter(|l| !noise(l)).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Starts `MultiServer.py` on `multi` in its own process group, its stdin
/// fed from `fifo`.
#[cfg(unix)]
fn serve(ap: &Archipelago, multi: &Path, port: u16, fifo: &Path, log: &Path) -> Result<u32> {
    use std::os::unix::process::CommandExt;
    let script = r#"tail -f "$1" | exec "$2" MultiServer.py --port "$3" "$4" > "$5" 2>&1"#;
    let child = Command::new("sh")
        .current_dir(&ap.worktree)
        .env("PYTHONUNBUFFERED", "1")
        .args(["-c", script, "sh"])
        .arg(fifo)
        .arg(ap.python())
        .arg(port.to_string())
        .arg(multi)
        .arg(log)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .context("starting MultiServer.py")?;
    Ok(child.id())
}

#[cfg(not(unix))]
fn serve(_: &Archipelago, _: &Path, _: u16, _: &Path, _: &Path) -> Result<u32> {
    bail!("rooms run on macOS and Linux")
}

#[cfg(unix)]
fn symlink(from: &Path, to: &Path) -> Result<()> {
    Ok(std::os::unix::fs::symlink(from, to)?)
}

#[cfg(not(unix))]
fn symlink(_: &Path, _: &Path) -> Result<()> {
    bail!("rooms run on macOS and Linux")
}

fn run(cmd: &mut Command) -> Result<()> {
    output(cmd).map(drop)
}

/// Runs `cmd`, its stdout on success, else its stderr's tail as the error.
fn output(cmd: &mut Command) -> Result<String> {
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("{cmd:?}"))?;
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    ensure!(
        out.status.success(),
        "{:?} failed:\n{}",
        cmd.get_program(),
        tail(&text(&out.stderr), 15)
    );
    Ok(text(&out.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exported options: id, kind, and a choice's values.
    fn exported() -> Vec<(String, Option<Vec<String>>)> {
        let text = include_str!("../../../../ap/znx/data/options.json");
        let json: Vec<serde_json::Value> = serde_json::from_str(text).unwrap();
        json.iter()
            .map(|e| {
                let values = e["options"].as_array().map(|o| {
                    o.iter()
                        .map(|v| v["id"].as_str().unwrap().to_owned())
                        .collect()
                });
                (e["id"].as_str().unwrap().to_owned(), values)
            })
            .collect()
    }

    #[test]
    fn each_preset_names_only_options_the_world_offers_at_values_it_takes() {
        let exported = exported();
        for (name, preset) in presets().unwrap() {
            for (id, value) in preset {
                let (_, values) = (exported.iter().find(|(e, _)| *e == id))
                    .unwrap_or_else(|| panic!("{name}.{id} isn't an AP option"));
                match (values, value) {
                    (None, toml::Value::Boolean(_)) => {}
                    (Some(vs), toml::Value::String(v)) => {
                        assert!(vs.contains(&v), "{name}.{id} = {v}: not one of {vs:?}")
                    }
                    (_, v) => panic!("{name}.{id} = {v}: the wrong kind"),
                }
            }
        }
    }

    #[test]
    fn all_on_names_every_option_the_world_offers() {
        let all = presets().unwrap().remove("all-on").unwrap();
        for (id, _) in exported() {
            assert!(all.contains_key(&id), "all-on leaves out {id}");
        }
    }

    #[test]
    fn a_player_file_holds_the_preset_and_the_test_start() {
        let yaml = player_yaml("Link", &options("defaults").unwrap());
        assert!(yaml.starts_with("name: Link\ngame: Ocarina of Time (znx)\n"));
        assert!(yaml.contains("\n  open_forest: true\n"));
        assert!(yaml.contains("\n  start_inventory_from_pool:\n    Kokiri Sword: 1\n"));
    }

    #[test]
    fn an_unknown_preset_names_the_ones_there_are() {
        let err = options("everything").unwrap_err().to_string();
        assert_eq!(err, "no preset everything: all-on, defaults");
    }

    #[test]
    fn a_send_is_one_cheat_console_line() {
        assert_eq!(
            send_line("Link", "Forest Medallion"),
            "/send Link Forest Medallion\n"
        );
    }

    #[test]
    fn the_log_tail_leaves_out_the_public_address_and_warnings() {
        let log = "a\nHosting game at 203.0.113.9:38281 (No password)\nb\nx.py:7: UserWarning: old\n  import y\nc";
        assert_eq!(tail(log, 2), "b\nc");
        assert_eq!(tail(log, 9), "a\nb\nc");
    }

    #[test]
    fn the_room_round_trips_through_its_file() {
        let home = std::env::temp_dir().join(format!("zootr-room-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        assert_eq!(Room::load(&home).unwrap(), None);
        let room = Room {
            pid: 7,
            port: PORT,
            seed: SEED,
            preset: "defaults".into(),
            generated: Generated {
                multi: "AP_1.archipelago".into(),
                slots: vec!["AP_1_P1_Link.apznx".into(), "AP_1_P2_Zelda.apznx".into()],
            },
            fifo: home.join("server.in"),
            log: home.join("server.log"),
        };
        room.save(&home).unwrap();
        assert_eq!(Room::load(&home).unwrap(), Some(room));
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_generation_s_files_are_found_by_slot() {
        let out = std::env::temp_dir().join(format!("zootr-gen-{}", std::process::id()));
        std::fs::create_dir_all(&out).unwrap();
        for f in [
            "AP_5.archipelago",
            "AP_5_P1_Link.apznx",
            "AP_5_P2_Zelda.apznx",
            "AP_5_Spoiler.txt",
        ] {
            std::fs::write(out.join(f), "").unwrap();
        }
        let g = Generated::find(&out).unwrap();
        assert_eq!(g.multi, out.join("AP_5.archipelago"));
        assert_eq!(
            g.slots,
            [
                out.join("AP_5_P1_Link.apznx"),
                out.join("AP_5_P2_Zelda.apznx")
            ]
        );
        std::fs::remove_dir_all(&out).unwrap();
    }
}
