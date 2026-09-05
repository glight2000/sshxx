//! Native metadata handling is isolated here; failures abort before publication.
#![allow(unsafe_code)]

use std::fs::{File, Metadata};
use std::path::Path;

use anyhow::{ensure, Result};
use tempfile::NamedTempFile;

pub(super) fn validate(info: &Metadata) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            info.nlink() == 1,
            "safe replacement would break hard links; edit this file outside sshxx"
        );
    }
    ensure!(!info.permissions().readonly(), "file is read-only");
    Ok(())
}

pub(super) fn unchanged(before: &Metadata, after: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (
            before.dev(),
            before.ino(),
            before.ctime(),
            before.ctime_nsec(),
        ) != (after.dev(), after.ino(), after.ctime(), after.ctime_nsec())
        {
            return false;
        }
    }
    after.is_file()
        && before.len() == after.len()
        && before.modified().ok() == after.modified().ok()
}

#[cfg(unix)]
pub(super) fn copy(source: &File, target: &File) -> Result<()> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::MetadataExt;
    let original = source.metadata()?;
    let staged = target.metadata()?;
    if original.uid() != staged.uid() || original.gid() != staged.gid() {
        // SAFETY: live borrowed file descriptor; IDs are from fstat.
        if unsafe { nix::libc::fchown(target.as_raw_fd(), original.uid(), original.gid()) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    target.set_permissions(original.permissions())?;
    #[cfg(target_os = "linux")]
    copy_xattrs(source, target)?;
    #[cfg(target_os = "macos")]
    {
        // SAFETY: descriptors remain open throughout this synchronous call.
        if unsafe {
            nix::libc::fcopyfile(
                source.as_raw_fd(),
                target.as_raw_fd(),
                std::ptr::null_mut(),
                nix::libc::COPYFILE_METADATA,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        // Metadata copying includes the old mtime; this save changed contents.
        target.set_times(std::fs::FileTimes::new().set_modified(std::time::SystemTime::now()))?;
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    anyhow::bail!("safe attribute preservation is not supported on this platform");
    Ok(())
}

#[cfg(target_os = "linux")]
fn copy_xattrs(source: &File, target: &File) -> Result<()> {
    use nix::libc;
    use std::ffi::{CStr, CString};
    use std::os::fd::AsRawFd;

    fn checked(result: isize) -> Result<usize> {
        if result < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let size = result as usize;
        ensure!(
            size <= 1 << 20,
            "file attributes exceed the safe copy limit"
        );
        Ok(size)
    }
    fn names(file: &File) -> Result<Vec<CString>> {
        // SAFETY: null/zero queries size; allocated buffer bounds the second call.
        let size = unsafe { libc::flistxattr(file.as_raw_fd(), std::ptr::null_mut(), 0) };
        if size < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOTSUP) {
            return Ok(Vec::new());
        }
        let mut bytes = vec![0u8; checked(size)?];
        let count = checked(unsafe {
            libc::flistxattr(file.as_raw_fd(), bytes.as_mut_ptr().cast(), bytes.len())
        })?;
        ensure!(
            count <= bytes.len(),
            "file attribute names changed while saving"
        );
        bytes.truncate(count);
        bytes
            .split_inclusive(|byte| *byte == 0)
            .map(|name| Ok(CStr::from_bytes_with_nul(name)?.to_owned()))
            .collect()
    }
    let original = names(source)?;
    for name in names(target)? {
        if !original.contains(&name) {
            // SAFETY: CString is terminated and descriptor is live.
            checked(unsafe { libc::fremovexattr(target.as_raw_fd(), name.as_ptr()) } as isize)?;
        }
    }
    for name in original {
        // SAFETY: names are NUL-terminated; all buffers have explicit bounds.
        let size = checked(unsafe {
            libc::fgetxattr(source.as_raw_fd(), name.as_ptr(), std::ptr::null_mut(), 0)
        })?;
        let mut value = vec![0u8; size];
        let count = checked(unsafe {
            libc::fgetxattr(
                source.as_raw_fd(),
                name.as_ptr(),
                value.as_mut_ptr().cast(),
                value.len(),
            )
        })?;
        ensure!(count <= value.len(), "file attributes changed while saving");
        checked(unsafe {
            libc::fsetxattr(
                target.as_raw_fd(),
                name.as_ptr(),
                value.as_ptr().cast(),
                count,
                0,
            )
        } as isize)?;
    }
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn replace(temporary: NamedTempFile, path: &Path) -> Result<()> {
    temporary.persist(path)?;
    File::open(path.parent().expect("staged files have a parent"))?
        .sync_all()
        .map_err(|error| anyhow::anyhow!("file was replaced but directory sync failed: {error}"))?;
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;

    #[test]
    fn preserves_extended_attributes() -> Result<()> {
        let source = tempfile::tempfile()?;
        let target = tempfile::tempfile()?;
        let name = c"user.sshxx-test";
        let value = b"preserve this attribute";
        // SAFETY: live descriptors, static C string and correctly sized buffers.
        let result = unsafe {
            nix::libc::fsetxattr(
                source.as_raw_fd(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        copy(&source, &target)?;
        let mut actual = [0u8; 64];
        let size = unsafe {
            nix::libc::fgetxattr(
                target.as_raw_fd(),
                name.as_ptr(),
                actual.as_mut_ptr().cast(),
                actual.len(),
            )
        };
        assert_eq!(size, value.len() as isize);
        assert_eq!(&actual[..value.len()], value);
        Ok(())
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn ReplaceFileW(
        replaced: *const u16,
        replacement: *const u16,
        backup: *const u16,
        flags: u32,
        exclude: *mut std::ffi::c_void,
        reserved: *mut std::ffi::c_void,
    ) -> i32;
    fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, info: *mut [u32; 13]) -> i32;
}

#[cfg(windows)]
pub(super) fn copy(source: &File, _target: &File) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    let mut info = [0u32; 13]; // BY_HANDLE_FILE_INFORMATION, all fields DWORD/FILETIME.
                               // SAFETY: buffer has the exact native layout; handle remains borrowed/open.
    if unsafe { GetFileInformationByHandle(source.as_raw_handle(), &mut info) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    ensure!(
        info[10] == 1,
        "safe replacement would break hard links; edit this file outside sshxx"
    );
    // ReplaceFileW merges ACLs and named streams at publication; do not use
    // IGNORE_MERGE_ERRORS or IGNORE_ACL_ERRORS flags.
    Ok(())
}

#[cfg(windows)]
pub(super) fn replace(temporary: NamedTempFile, path: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    // ReplaceFileW opens the replacement without sharing. Close our handle first.
    let temporary = temporary.into_temp_path();
    let backup_path = path.with_file_name(format!(
        ".sshxx-save-backup-{}",
        sshx_core::rand_alphanumeric(24)
    ));
    let backup = backup_path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let staged = temporary
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: NUL-terminated UTF-16 buffers remain live; optional pointers are null.
    if unsafe {
        ReplaceFileW(
            target.as_ptr(),
            staged.as_ptr(),
            backup.as_ptr(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        let error = std::io::Error::last_os_error();
        // Without a backup, ERROR_UNABLE_TO_MOVE_REPLACEMENT can delete the
        // original. Retain the original backup and staged contents on failure.
        let staged_path = temporary.keep()?;
        anyhow::bail!("Windows file replacement failed: {error}. Check the original and recovery files {} and {} before retrying",
            backup_path.display(), staged_path.display());
    }
    std::fs::remove_file(&backup_path).map_err(|error| {
        anyhow::anyhow!(
            "file was saved, but backup cleanup failed at {}: {error}",
            backup_path.display()
        )
    })?;
    Ok(())
}
