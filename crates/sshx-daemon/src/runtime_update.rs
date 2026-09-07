//! Opt-in bridge to an administrator-provisioned systemd update job.
//! No browser-controlled commands, service names, paths or arguments.
use anyhow::{bail, Context, Result};
use std::path::Path;
use tokio::{
    process::Command,
    time::{timeout, Duration},
};

const UNIT: &str = "sshxx-update.service";

pub(crate) fn scope() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    std::env::var("SSHXX_WEB_UPDATE")
        .ok()
        .filter(|value| matches!(value.as_str(), "user" | "system"))
}

/// The running image's archive, not the installer's newly selected pointer.
pub(crate) fn release_from_executable(path: &Path) -> Option<String> {
    let bin = path.parent()?;
    let version = bin.parent()?;
    if bin.file_name()? != "bin" || version.parent()?.file_name()? != "versions" {
        return None;
    }
    let value = version.file_name()?.to_str()?;
    let parts: Vec<_> = value.split('.').collect();
    (parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit())))
    .then(|| value.to_owned())
}

pub(crate) fn running_release() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|path| release_from_executable(&path))
        .unwrap_or_else(|| "source build".into())
}

async fn control(scope: &str, start: bool) -> Result<String> {
    let mut command = if start && scope == "system" {
        let mut command = Command::new("/usr/bin/sudo");
        command.args(["-n", "--", "/usr/bin/systemctl"]);
        command
    } else {
        Command::new("/usr/bin/systemctl")
    };
    if scope == "user" {
        command.arg("--user");
    }
    if start {
        command.args(["restart", "--no-block", "--job-mode=fail", UNIT]);
    } else {
        command.args([
            "show",
            UNIT,
            "--property=LoadState,Type,RemainAfterExit,ActiveState,SubState,Result",
        ]);
    }
    let output = timeout(
        Duration::from_secs(4),
        command
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .context("Update service request timed out")??;
    if !output.status.success() {
        bail!("Update service unavailable or permission denied; ask the administrator to configure sshxx-update.service");
    }
    Ok(String::from_utf8(output.stdout)?)
}

pub(crate) fn describe_status(properties: &str) -> &'static str {
    let has = |key: &str| properties.lines().any(|line| line == key);
    if !has("LoadState=loaded") {
        return "not configured";
    }
    if !has("Type=oneshot") || !has("RemainAfterExit=yes") {
        return "not configured: requires Type=oneshot and RemainAfterExit=yes";
    }
    if has("ActiveState=activating") || has("ActiveState=deactivating") {
        return "updating";
    }
    if has("ActiveState=failed")
        || (!has("Result=success") && properties.lines().any(|l| l.starts_with("Result=")))
    {
        return "failed; inspect sshxx-update.service logs";
    }
    if has("ActiveState=active") && has("SubState=exited") {
        return "completed; reload this viewer to use the updated Web client";
    }
    "ready"
}

pub(crate) async fn status() -> String {
    let Some(scope) = scope() else {
        return "not enabled".into();
    };
    match control(&scope, false).await {
        Ok(properties) => describe_status(&properties).into(),
        Err(_) => "unavailable; check update service permissions".into(),
    }
}

pub(crate) async fn start() -> Result<()> {
    let scope = scope().context("Web updates are not enabled by the administrator")?;
    let properties = control(&scope, false).await?;
    if describe_status(&properties).starts_with("not configured") {
        bail!("sshxx-update.service requires administrator setup with Type=oneshot and RemainAfterExit=yes");
    }
    if describe_status(&properties) == "updating" {
        return Ok(());
    }
    // Retain completed results with RemainAfterExit=yes. Never restart an
    // activating job; fail rather than replace a concurrently queued system job.
    control(&scope, true).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versions_and_job_states_are_not_inferred_from_installed_files() {
        assert_eq!(
            release_from_executable(Path::new("/runtime/versions/0.13.2/bin/sshxx-daemon"))
                .as_deref(),
            Some("0.13.2")
        );
        assert!(release_from_executable(Path::new("/target/debug/sshxx-daemon")).is_none());
        assert!(
            release_from_executable(Path::new("/runtime/versions/current/bin/sshxx-daemon"))
                .is_none()
        );
        let status = |state: &str| {
            describe_status(&format!(
                "LoadState=loaded\nType=oneshot\nRemainAfterExit=yes\n{state}"
            ))
        };
        assert_eq!(status("ActiveState=activating\nResult=success"), "updating");
        assert!(status("ActiveState=failed\nResult=exit-code").starts_with("failed"));
        assert!(
            status("ActiveState=active\nSubState=exited\nResult=success").starts_with("completed")
        );
        assert_eq!(describe_status("LoadState=not-found"), "not configured");
        assert_eq!(status("ActiveState=inactive\nResult=success"), "ready");
        assert!(
            describe_status("LoadState=loaded\nType=oneshot\nRemainAfterExit=no")
                .starts_with("not configured")
        );
    }
}
