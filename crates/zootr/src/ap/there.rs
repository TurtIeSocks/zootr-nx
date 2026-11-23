//! Another machine of the turn queue, reached over ssh: a room hosted
//! there, or the whole bench run there. Where it is lives in the config,
//! never in the repo.

use std::io::Write;
use std::process::{Child, ChildStdin, Command, Output, Stdio};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

/// How to reach a machine of the turn queue, under `machines.NAME`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Machine {
    /// The ssh destination.
    pub ssh: String,
    /// The repo checkout there, with a release build.
    pub checkout: String,
    /// Its Archipelago checkout (`ap room setup` makes it).
    pub archipelago: String,
    /// The address other machines reach its rooms at.
    pub host: String,
}

/// The variables a command there gets from here when they're set.
const PASSED: [&str; 2] = ["ZOOTR_SESSION", "CLAUDECODE"];

/// A configured machine.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct There {
    pub name: String,
    pub machine: Machine,
}

impl There {
    pub fn find(name: &str) -> Result<Self> {
        let config = crate::config::Config::load()?;
        let Some(machine) = config.machines.get(name) else {
            bail!(
                "no `machines.{name}` in the config: set its ssh, checkout, archipelago and host"
            );
        };
        ensure!(!machine.ssh.is_empty(), "`machines.{name}.ssh` is empty");
        Ok(Self {
            name: name.to_owned(),
            machine: machine.clone(),
        })
    }

    /// `script` run there by sh.
    pub fn sh(&self, script: &str) -> Command {
        let mut cmd = Command::new("ssh");
        cmd.args(["-o", "BatchMode=yes", &self.machine.ssh, "--", script]);
        cmd
    }

    /// The checkout's release `zootr` there, with `args`.
    pub fn zootr(&self, args: &[&str]) -> Command {
        let passed = PASSED
            .iter()
            .filter_map(|&k| Some((k, std::env::var(k).ok()?)));
        self.sh(&self.zootr_line(args, passed.collect()))
    }

    fn zootr_line(&self, args: &[&str], passed: Vec<(&str, String)>) -> String {
        let env: Vec<String> = (passed.iter())
            .map(|(k, v)| format!("{k}={}", quote(v)))
            .chain([format!(
                "ZOOTR_ARCHIPELAGO={}",
                path(&self.machine.archipelago)
            )])
            .collect();
        let args: Vec<String> = args.iter().map(|a| quote(a)).collect();
        format!(
            "cd {} && {} target/release/zootr {}",
            path(&self.machine.checkout),
            env.join(" "),
            args.join(" ")
        )
    }

    /// Runs `args` there and returns its output, failing with it.
    pub fn run(&self, args: &[&str]) -> Result<String> {
        checked(self.zootr(args).output(), &args.join(" "))
    }

    /// The file at `path` there.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let out = self.sh(&format!("cat {}", quote(path))).output()?;
        ensure!(
            out.status.success(),
            "reading {path} on {}: {}",
            self.name,
            err(&out)
        );
        Ok(out.stdout)
    }

    /// A pipe into the file (a FIFO) at `path` there, kept open across
    /// writes so each costs no new connection.
    pub fn pipe(&self, path: &str) -> Result<Pipe> {
        let child = (self.sh(&format!("cat > {}", quote(path))))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .context("ssh")?;
        Ok(Pipe(child))
    }

    /// Clones Archipelago at `rev` there and makes its venv with the
    /// machine's `uv`, every world's requirements installed.
    pub fn setup(&self, rev: &str) -> Result<()> {
        let script = setup_script(&self.machine.archipelago, rev);
        let out = self.sh(&script).output()?;
        ensure!(
            out.status.success(),
            "setup on {} failed:\n{}",
            self.name,
            err(&out)
        );
        Ok(())
    }
}

/// An open pipe into a file there.
pub struct Pipe(Child);

impl Pipe {
    pub fn write(&mut self, line: &str) -> Result<()> {
        let stdin: &mut ChildStdin = self.0.stdin.as_mut().context("the pipe is closed")?;
        stdin.write_all(line.as_bytes())?;
        Ok(stdin.flush()?)
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        drop(self.0.stdin.take());
        self.0.wait().ok();
    }
}

fn setup_script(ap: &str, rev: &str) -> String {
    let (ap, rev) = (path(ap), quote(rev));
    format!(
        r#"set -e
export PATH="$HOME/.local/bin:$PATH"
command -v uv >/dev/null || {{ echo "uv isn't installed there" >&2; exit 1; }}
[ -d {ap}/.git ] || git clone -q https://github.com/ArchipelagoMW/Archipelago.git {ap}
git -C {ap} fetch -q origin
git -C {ap} checkout -q --detach {rev}
[ -x {ap}/.venv/bin/python ] || uv venv -q --seed --python '>=3.11,<3.14' {ap}/.venv
cd {ap} && .venv/bin/python ModuleUpdate.py --yes >/dev/null"#
    )
}

fn checked(out: std::io::Result<Output>, what: &str) -> Result<String> {
    let out = out.context("ssh")?;
    ensure!(
        out.status.success(),
        "zootr {what} failed there:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        err(&out)
    );
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn err(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// `p` as one sh word, a leading `~/` still meaning home.
fn path(p: &str) -> String {
    match p.strip_prefix("~/") {
        Some(rest) => format!("\"$HOME\"/{}", quote(rest)),
        None => quote(p),
    }
}

/// `s` as one sh word.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn there() -> There {
        There {
            name: "far".into(),
            machine: Machine {
                ssh: "far".into(),
                checkout: "~/zootr".into(),
                archipelago: "/ap dir".into(),
                host: "10.0.0.2".into(),
            },
        }
    }

    #[test]
    fn words_are_quoted_for_sh() {
        assert_eq!(quote("Red Rupee (20)"), "'Red Rupee (20)'");
        assert_eq!(quote("Link's"), r"'Link'\''s'");
    }

    #[test]
    fn a_zootr_line_runs_the_release_build_in_the_checkout_with_archipelago() {
        let passed = vec![("ZOOTR_SESSION", "bench".to_string())];
        let line = there().zootr_line(&["ap", "room", "send", "Red Rupee (20)"], passed);
        assert_eq!(
            line,
            "cd \"$HOME\"/'zootr' && ZOOTR_SESSION='bench' ZOOTR_ARCHIPELAGO='/ap dir' \
             target/release/zootr 'ap' 'room' 'send' 'Red Rupee (20)'"
        );
    }

    #[test]
    fn setup_clones_once_and_checks_out_the_revision() {
        let s = setup_script("/ap", "54803be");
        assert!(s.contains("[ -d '/ap'/.git ] || git clone"));
        assert!(s.contains("checkout -q --detach '54803be'"));
        assert!(s.contains("ModuleUpdate.py --yes"));
    }
}
