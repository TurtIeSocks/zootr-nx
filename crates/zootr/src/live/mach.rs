//! The Mac game's memory through Mach, and a stub elsewhere.

#[cfg(target_os = "macos")]
mod imp {
    use std::path::Path;
    use std::process::Command;

    use anyhow::{Result, bail};

    pub fn pids(app: &Path) -> Result<Vec<i32>> {
        let out = Command::new("ps").args(["-axo", "pid=,comm="]).output()?;
        let app = app.canonicalize().unwrap_or_else(|_| app.to_path_buf());
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let (pid, exe) = l.trim().split_once(' ')?;
                Path::new(exe.trim())
                    .starts_with(&app)
                    .then(|| pid.parse().ok())?
            })
            .collect())
    }

    type Port = u32;
    const BASIC_INFO_64: i32 = 9;

    unsafe extern "C" {
        static mach_task_self_: Port;
        fn task_for_pid(target: Port, pid: i32, task: *mut Port) -> i32;
        fn mach_vm_read_overwrite(task: Port, at: u64, size: u64, data: u64, got: *mut u64) -> i32;
        fn mach_vm_write(task: Port, at: u64, data: usize, count: u32) -> i32;
        fn mach_vm_region(
            task: Port,
            at: *mut u64,
            size: *mut u64,
            flavor: i32,
            info: *mut i32,
            count: *mut u32,
            object: *mut Port,
        ) -> i32;
    }

    pub fn task(pid: i32) -> Option<Port> {
        let mut task = 0;
        let kr = unsafe { task_for_pid(mach_task_self_, pid, &mut task) };
        (kr == 0).then_some(task)
    }

    pub fn read(task: Port, at: u64, len: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0; len];
        if len == 0 {
            return Ok(buf);
        }
        let mut got = 0;
        let kr = unsafe {
            mach_vm_read_overwrite(task, at, len as u64, buf.as_mut_ptr() as u64, &mut got)
        };
        if kr != 0 || got != len as u64 {
            bail!("reading {len} bytes at {at:#x} failed ({kr})");
        }
        Ok(buf)
    }

    pub fn write(task: Port, at: u64, data: &[u8]) -> Result<()> {
        let kr = unsafe { mach_vm_write(task, at, data.as_ptr() as usize, data.len() as u32) };
        if kr != 0 {
            bail!("writing {} bytes at {at:#x} failed ({kr})", data.len());
        }
        Ok(())
    }

    /// Every mapped region, lowest first: its start, size and protection
    /// (read 1, write 2, execute 4).
    pub fn regions(task: Port) -> Vec<(u64, usize, i32)> {
        let mut out = Vec::new();
        let mut at = 0;
        loop {
            let (mut size, mut info, mut count, mut object) = (0, [0i32; 9], 9, 0);
            let kr = unsafe {
                mach_vm_region(
                    task,
                    &mut at,
                    &mut size,
                    BASIC_INFO_64,
                    info.as_mut_ptr(),
                    &mut count,
                    &mut object,
                )
            };
            if kr != 0 {
                return out;
            }
            out.push((at, size as usize, info[0]));
            at += size;
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::path::Path;

    use anyhow::{Result, bail};

    pub fn pids(_: &Path) -> Result<Vec<i32>> {
        Err(crate::live::Unsupported.into())
    }

    pub fn task(_: i32) -> Option<u32> {
        None
    }

    pub fn read(_: u32, _: u64, _: usize) -> Result<Vec<u8>> {
        bail!("live reads need macOS, or a target with a bridge")
    }

    pub fn write(_: u32, _: u64, _: &[u8]) -> Result<()> {
        bail!("live writes need macOS, or a target with a bridge")
    }

    pub fn regions(_: u32) -> Vec<(u64, usize, i32)> {
        Vec::new()
    }
}

pub use imp::*;

/// Readable, executable regions, lowest first.
pub fn code(task: u32) -> Vec<(u64, usize)> {
    (regions(task).into_iter())
        .filter(|&(_, _, prot)| prot & 5 == 5)
        .map(|(at, size, _)| (at, size))
        .collect()
}
