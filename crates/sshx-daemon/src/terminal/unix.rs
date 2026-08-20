use std::convert::Infallible;
use std::env;
use std::ffi::{CStr, CString};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::Result;
use close_fds::CloseFdsBuilder;
use nix::errno::Errno;
use nix::libc::{login_tty, TIOCGWINSZ, TIOCSWINSZ};
use nix::pty::{self, Winsize};
use nix::sys::signal::{kill, Signal::SIGKILL};
use nix::sys::wait::waitpid;
use nix::unistd::{execvp, fork, ForkResult, Pid};
use pin_project::{pin_project, pinned_drop};
use tokio::fs::{self, File};
use tokio::io::{self, AsyncRead, AsyncWrite};
use tracing::{instrument, trace};

/// Returns the default shell on this system.
pub async fn get_default_shell() -> String {
    if let Ok(shell) = env::var("SHELL") {
        if !shell.is_empty() {
            return shell;
        }
    }
    for shell in [
        "/bin/bash",
        "/bin/sh",
        "/usr/local/bin/bash",
        "/usr/local/bin/sh",
    ] {
        if fs::metadata(shell).await.is_ok() {
            return shell.to_string();
        }
    }
    String::from("sh")
}

/// An object that stores the state for a terminal session.
#[pin_project(PinnedDrop)]
pub struct Terminal {
    child: Pid,
    #[pin]
    master_read: File,
    #[pin]
    master_write: File,
}

impl Terminal {
    /// Create a new terminal, with attached PTY.
    #[instrument]
    pub async fn new(
        program: &str,
        args: &[String],
        working_directory: Option<&Path>,
    ) -> Result<Terminal> {
        let result = pty::openpty(None, None)?;

        // The slave file descriptor was created by openpty() and is forked here.
        let child = Self::fork_child(program, args, result.slave.as_raw_fd(), working_directory)?;

        // We need to clone the file object to prevent livelocks in Tokio, when multiple
        // reads and writes happen concurrently on the same file descriptor. This is a
        // current limitation of how the `tokio::fs::File` struct is implemented, due to
        // its blocking I/O on a separate thread.
        let master_read = File::from(std::fs::File::from(result.master));
        let master_write = master_read.try_clone().await?;

        trace!(%child, "creating new terminal");

        Ok(Self {
            child,
            master_read,
            master_write,
        })
    }

    /// Entry point for the child process, which spawns a shell.
    fn fork_child(
        program: &str,
        args: &[String],
        slave_port: RawFd,
        working_directory: Option<&Path>,
    ) -> Result<Pid> {
        let program = CString::new(program.to_owned())?;
        let mut argv = Vec::with_capacity(args.len() + 1);
        argv.push(program.clone());
        argv.extend(
            args.iter()
                .map(|arg| CString::new(arg.as_str()))
                .collect::<Result<Vec<_>, _>>()?,
        );
        let working_directory = working_directory
            .map(|path| CString::new(path.as_os_str().as_bytes()))
            .transpose()?;

        // Safety: This does not use any async-signal-unsafe operations in the child
        // branch, such as memory allocation.
        match unsafe { fork() }? {
            ForkResult::Parent { child } => Ok(child),
            ForkResult::Child => {
                match Self::execv_child(&program, &argv, slave_port, working_directory.as_deref()) {
                    Ok(infallible) => match infallible {},
                    Err(_) => std::process::exit(1),
                }
            }
        }
    }

    fn execv_child(
        program: &CStr,
        argv: &[CString],
        slave_port: RawFd,
        working_directory: Option<&CStr>,
    ) -> Result<Infallible, Errno> {
        // Safety: The slave file descriptor was created by openpty().
        Errno::result(unsafe { login_tty(slave_port) })?;
        // Safety: This is called immediately before an execv(), and there are no other
        // threads in this process to interact with its file descriptor table.
        unsafe { CloseFdsBuilder::new().closefrom(3) };

        if let Some(working_directory) = working_directory {
            // Safety: `working_directory` is a valid, null-terminated path.
            Errno::result(unsafe { nix::libc::chdir(working_directory.as_ptr()) })?;
        }

        // Set terminal environment variables appropriately.
        env::set_var("TERM", "xterm-256color");
        env::set_var("COLORTERM", "truecolor");
        env::set_var("TERM_PROGRAM", "sshxx-daemon");
        env::remove_var("TERM_PROGRAM_VERSION");

        // Start the process.
        execvp(program, argv)
    }

    /// Get the window size of the TTY.
    pub fn get_winsize(&self) -> Result<(u16, u16)> {
        nix::ioctl_read_bad!(ioctl_get_winsize, TIOCGWINSZ, Winsize);
        let mut winsize = make_winsize(0, 0);
        // Safety: The master file descriptor was created by openpty().
        unsafe { ioctl_get_winsize(self.master_read.as_raw_fd(), &mut winsize) }?;
        Ok((winsize.ws_row, winsize.ws_col))
    }

    /// Set the window size of the TTY.
    pub fn set_winsize(&mut self, rows: u16, cols: u16) -> Result<()> {
        nix::ioctl_write_ptr_bad!(ioctl_set_winsize, TIOCSWINSZ, Winsize);
        let winsize = make_winsize(rows, cols);
        // Safety: The master file descriptor was created by openpty().
        unsafe { ioctl_set_winsize(self.master_read.as_raw_fd(), &winsize) }?;
        Ok(())
    }

    /// Return the current working directory of the shell process when available.
    pub async fn working_directory(&self) -> Option<PathBuf> {
        fs::read_link(format!("/proc/{}/cwd", self.child.as_raw()))
            .await
            .ok()
    }
}

// Redirect terminal reads to the read file object.
impl AsyncRead for Terminal {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.project().master_read.poll_read(cx, buf)
    }
}

// Redirect terminal writes to the write file object.
impl AsyncWrite for Terminal {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.project().master_write.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().master_write.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().master_write.poll_shutdown(cx)
    }
}

#[pinned_drop]
impl PinnedDrop for Terminal {
    fn drop(self: Pin<&mut Self>) {
        let this = self.project();
        let child = *this.child;
        trace!(%child, "dropping terminal");

        // Kill the child process on closure so that it doesn't keep running.
        kill(child, SIGKILL).ok();

        // Reap the zombie process in a background thread.
        std::thread::spawn(move || {
            waitpid(child, None).ok();
        });
    }
}

fn make_winsize(rows: u16, cols: u16) -> Winsize {
    Winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0, // ignored
        ws_ypixel: 0, // ignored
    }
}
