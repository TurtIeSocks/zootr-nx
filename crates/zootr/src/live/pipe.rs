//! The bridge on Windows: the runner's named pipe. Windows runs blocking
//! reads and writes on one pipe handle one at a time, so a read that waits
//! inside the system would hold up the next request. A read here peeks
//! until bytes are there, and only then takes them.

use std::cell::Cell;
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::ptr::null_mut;
use std::time::{Duration, Instant};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn PeekNamedPipe(
        pipe: *mut c_void,
        buffer: *mut c_void,
        size: u32,
        read: *mut u32,
        available: *mut u32,
        left: *mut u32,
    ) -> i32;
    fn WaitNamedPipeW(name: *const u16, timeout_ms: u32) -> i32;
}

const PREFIX: &str = r"\\.\pipe\";
/// The runner closed its end.
const ERROR_BROKEN_PIPE: i32 = 109;
/// Every instance has a client: the runner is making the next one.
const ERROR_PIPE_BUSY: i32 = 231;
/// How long a connect waits for a free instance.
const BUSY_WAIT_MS: u32 = 2000;
/// How long a read spins before it starts sleeping between peeks. A reply
/// usually lands within it, and the system's sleeps are coarse.
const SPIN: Duration = Duration::from_millis(2);

/// `path` as a pipe's full name. The runner prefixes a bare name the same
/// way, so either spelling in the target reaches the same pipe.
pub fn name(path: &Path) -> String {
    let path = path.to_string_lossy();
    match path.starts_with(PREFIX) {
        true => path.into_owned(),
        false => format!("{PREFIX}{path}"),
    }
}

/// One end of the runner's pipe, read and written like a socket.
pub struct Pipe {
    file: File,
    timeout: Cell<Option<Duration>>,
}

impl Pipe {
    pub fn connect(path: &Path) -> io::Result<Self> {
        let name = name(path);
        let open = || OpenOptions::new().read(true).write(true).open(&name);
        let file = match open() {
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
                unsafe { WaitNamedPipeW(wide.as_ptr(), BUSY_WAIT_MS) };
                open()?
            }
            file => file?,
        };
        Ok(Self::of(file))
    }

    fn of(file: File) -> Self {
        Self {
            file,
            timeout: Cell::new(None),
        }
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self::of(self.file.try_clone()?))
    }

    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.timeout.set(timeout);
        Ok(())
    }

    /// How many bytes wait in the pipe, or `None` once the runner closed it.
    fn waiting(&self) -> io::Result<Option<usize>> {
        let mut available = 0;
        let handle = self.file.as_raw_handle();
        let ok = unsafe {
            PeekNamedPipe(
                handle,
                null_mut(),
                0,
                null_mut(),
                &mut available,
                null_mut(),
            )
        };
        if ok != 0 {
            return Ok(Some(available as usize));
        }
        let e = io::Error::last_os_error();
        match e.raw_os_error() {
            Some(ERROR_BROKEN_PIPE) => Ok(None),
            _ => Err(e),
        }
    }
}

impl Read for Pipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let start = Instant::now();
        loop {
            match self.waiting()? {
                None => return Ok(0),
                Some(0) => {}
                Some(n) => {
                    let n = n.min(buf.len());
                    return self.file.read(&mut buf[..n]);
                }
            }
            let waited = start.elapsed();
            if self.timeout.get().is_some_and(|t| waited >= t) {
                return Err(io::ErrorKind::TimedOut.into());
            }
            match waited < SPIN {
                true => std::thread::yield_now(),
                false => std::thread::sleep(Duration::from_millis(1)),
            }
        }
    }
}

impl Write for Pipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::os::windows::io::FromRawHandle;

    use super::*;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateNamedPipeW(
            name: *const u16,
            open_mode: u32,
            pipe_mode: u32,
            instances: u32,
            out_size: u32,
            in_size: u32,
            timeout: u32,
            security: *mut c_void,
        ) -> *mut c_void;
        fn ConnectNamedPipe(pipe: *mut c_void, overlapped: *mut c_void) -> i32;
    }

    const PIPE_ACCESS_DUPLEX: u32 = 3;
    const ERROR_PIPE_CONNECTED: i32 = 535;

    /// The server end of a fresh byte pipe, the way the runner makes it.
    pub struct Server(File);

    pub fn serve(name: &str) -> Server {
        let wide: Vec<u16> = super::name(Path::new(name))
            .encode_utf16()
            .chain([0])
            .collect();
        let handle = unsafe {
            CreateNamedPipeW(
                wide.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                0,
                1,
                1 << 16,
                1 << 16,
                0,
                null_mut(),
            )
        };
        assert!(handle as isize != -1, "{}", io::Error::last_os_error());
        Server(unsafe { File::from_raw_handle(handle) })
    }

    impl Server {
        /// The pipe once its client is on: already, or when it comes.
        pub fn accept(self) -> File {
            let ok = unsafe { ConnectNamedPipe(self.0.as_raw_handle(), null_mut()) };
            let e = io::Error::last_os_error();
            assert!(
                ok != 0 || e.raw_os_error() == Some(ERROR_PIPE_CONNECTED),
                "{e}"
            );
            self.0
        }
    }

    #[test]
    fn a_closed_pipe_reads_as_the_end() {
        let name = format!("zootr-pipe-closed-{}", std::process::id());
        let server = serve(&name);
        let mut client = Pipe::connect(Path::new(&name)).unwrap();
        drop(server.accept());
        assert_eq!(client.read(&mut [0; 8]).unwrap(), 0);
    }

    #[test]
    fn a_bare_name_gets_the_pipe_prefix() {
        assert_eq!(name(Path::new("zootr")), r"\\.\pipe\zootr");
        assert_eq!(name(Path::new(r"\\.\pipe\zootr")), r"\\.\pipe\zootr");
    }
}
