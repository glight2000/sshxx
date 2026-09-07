//! Reload a runtime executable without introducing a second service manager.
//!
//! Targets come only from the running executable and the local install layout,
//! never from a browser request. Unix exec preserves the service PID/cgroup.
//! Windows launchers handle exit 75; direct executable launches spawn a successor.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use anyhow::{ensure, Context, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Restart {
    executable: PathBuf,
}

impl std::fmt::Display for Restart {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("runtime process restart requested")
    }
}

impl std::error::Error for Restart {}

impl Restart {
    /// Validate the replacement before acknowledging a destructive action.
    pub async fn prepare(role: &str) -> Result<Self> {
        let current = running_executable()?;
        let executable = resolve_executable(&current, role)?;
        let status = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::process::Command::new(&executable)
                .arg("--version")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .status(),
        )
        .await
        .context("replacement executable validation timed out")?
        .context("cannot run replacement executable; process was not restarted")?;
        ensure!(status.success(), "replacement executable validation failed");
        Ok(Self { executable })
    }

    /// Call only after the async runtime has shut down and released its handles.
    pub fn execute(self) -> Result<ExitCode> {
        let mut command = Command::new(self.executable);
        command.args(std::env::args_os().skip(1));
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            Err(command.exec()).context("could not reload runtime executable")
        }
        #[cfg(windows)]
        {
            if std::env::var_os("SSHXX_RESTART_SUPERVISED").as_deref()
                == Some(std::ffi::OsStr::new("1"))
            {
                return Ok(ExitCode::from(75));
            }
            command
                .spawn()
                .context("could not launch replacement runtime")?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn running_executable() -> Result<PathBuf> {
    let current = std::env::current_exe().context("cannot locate running executable")?;
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        // Atomic binary replacement leaves the running Linux image marked
        // "(deleted)". Reopen the installed path, never /proc/self/exe.
        if let Some(path) = current.as_os_str().as_bytes().strip_suffix(b" (deleted)") {
            return Ok(PathBuf::from(std::ffi::OsString::from_vec(path.to_vec())));
        }
    }
    Ok(current)
}

fn resolve_executable(current: &Path, role: &str) -> Result<PathBuf> {
    ensure!(
        matches!(role, "sshxx-daemon" | "sshxx-terminal-host"),
        "unsupported runtime role"
    );
    let name = format!("{role}{}", std::env::consts::EXE_SUFFIX);
    ensure!(
        current.file_name() == Some(std::ffi::OsStr::new(&name)),
        "process restart is only available in the standalone runtime executable"
    );
    // Official installers retain versions/<version>/bin/<role>. Resolve the
    // selected version again, instead of re-executing the old version's path.
    if let Some(versions) = current
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
    {
        if current.parent().and_then(Path::file_name) == Some(std::ffi::OsStr::new("bin"))
            && versions.file_name() == Some(std::ffi::OsStr::new("versions"))
        {
            let root = versions.parent().context("invalid runtime installation")?;
            let version = std::fs::read_to_string(root.join("current-version"))
                .context("cannot read selected runtime version")?;
            let version = version.trim();
            ensure!(
                !version.is_empty()
                    && version.len() <= 64
                    && version != "."
                    && version != ".."
                    && version
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b".-+".contains(&c)),
                "invalid selected runtime version"
            );
            return Ok(versions.join(version).join("bin").join(name));
        }
    }
    Ok(current.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_resolves_selected_version_without_accepting_arbitrary_paths() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let name = format!("sshxx-daemon{}", std::env::consts::EXE_SUFFIX);
        let current = directory.path().join("versions/old/bin").join(&name);
        std::fs::write(directory.path().join("current-version"), "0.14.0\n")?;
        assert_eq!(
            resolve_executable(&current, "sshxx-daemon")?,
            directory.path().join("versions/0.14.0/bin").join(&name)
        );
        for invalid in ["../escape", "..", "", "/tmp", "one\ntwo", "a\\b"] {
            std::fs::write(directory.path().join("current-version"), invalid)?;
            assert!(resolve_executable(&current, "sshxx-daemon").is_err());
        }
        let standalone = directory.path().join(name);
        assert_eq!(resolve_executable(&standalone, "sshxx-daemon")?, standalone);
        assert!(resolve_executable(&current, "sshxx-server").is_err());
        assert!(resolve_executable(&current, "sshxx-terminal-host").is_err());
        Ok(())
    }
}
