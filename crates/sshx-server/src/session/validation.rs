//! Validation and wire-format conversion at the session trust boundary.

use std::collections::HashSet;

use anyhow::{bail, Context, Result};
use sshx_core::{
    proto::{SshAuthMethod, SshProfile},
    Sid,
};

use crate::web::protocol::{
    WsCustomWindow, WsFileWindow, WsNote, WsSshAuthMethod, WsSshProfile, WsWinsize,
};

pub(super) fn validate_title(title: &str) -> Result<()> {
    if title.len() > 100 || title.chars().any(char::is_control) {
        bail!("window title is invalid");
    }
    Ok(())
}

pub(super) fn validate_file_window(window: &WsFileWindow) -> Result<()> {
    validate_title(&window.title)?;
    validate_color(&window.background)?;
    if window.path.is_empty()
        || window.path.len() > 16_384
        || window.path.contains('\0')
        || !(600..=4_000).contains(&window.width)
        || !(360..=4_000).contains(&window.height)
    {
        bail!("file browser state is invalid");
    }
    for path in std::iter::once(&window.current_path)
        .chain(window.expanded_paths.iter())
        .chain([&window.selected_path, &window.editor_path])
    {
        if path.len() > 16_384 || path.contains('\0') {
            bail!("file browser path state is invalid");
        }
    }
    if window.expanded_paths.len() > 512
        || window.expanded_paths.iter().map(String::len).sum::<usize>() > 256 << 10
        || !matches!(
            window.selected_kind.as_str(),
            "" | "directory" | "file" | "symlink" | "other"
        )
        || window.editor_data.len() > 8 << 20
        || !(200..=1_600).contains(&window.sidebar_width)
        || window.sidebar_width.saturating_add(320) > window.width
        || (!window.editor_data.is_empty() && window.editor_stream & (1 << 63) == 0)
        || (window.editor_path.is_empty()
            && (!window.editor_data.is_empty() || window.editor_dirty))
    {
        bail!("file browser shared view state is invalid");
    }
    Ok(())
}

pub(super) fn validate_file_editor_total(windows: &[(Sid, WsFileWindow)]) -> Result<()> {
    if windows
        .iter()
        .map(|(_, window)| window.editor_data.len())
        .sum::<usize>()
        > 48 << 20
    {
        bail!("shared file editor buffers exceed the session limit");
    }
    Ok(())
}

pub(super) fn validate_custom_window(window: &WsCustomWindow) -> Result<()> {
    // The 40-unit client grid uses a 4-unit inset on both window edges. These
    // limits therefore span exactly two horizontal and three vertical cells.
    const MIN_WIDTH: u16 = 2 * 40 - 2 * 4;
    const MIN_HEIGHT: u16 = 3 * 40 - 2 * 4;
    validate_title(&window.title)?;
    validate_color(&window.background)?;
    if window.url.len() > 4_096 || window.url.chars().any(char::is_control) {
        bail!("custom component URL is invalid");
    }
    if window.use_url && window.show_preview {
        let remainder = window
            .url
            .strip_prefix("https://")
            .or_else(|| window.url.strip_prefix("http://"));
        let authority = remainder
            .unwrap_or_default()
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default();
        if authority.is_empty() {
            bail!("custom component URL must be an absolute HTTP(S) URL");
        }
    }
    if !(MIN_WIDTH..=4_000).contains(&window.width)
        || !(MIN_HEIGHT..=4_000).contains(&window.height)
        || window.source.len() > 256 << 10
        || window.source.contains('\0')
    {
        bail!("custom component state is invalid");
    }
    Ok(())
}

pub(super) fn validate_custom_source_total(windows: &[(Sid, WsCustomWindow)]) -> Result<()> {
    if windows
        .iter()
        .map(|(_, window)| window.source.len())
        .sum::<usize>()
        > 4 << 20
    {
        bail!("custom component sources exceed the session limit");
    }
    Ok(())
}

pub(super) fn normalize_note_paragraphs(text: &str, paragraphs: Vec<String>) -> Vec<String> {
    if paragraphs.is_empty() {
        text.split('\n').map(str::to_owned).collect()
    } else {
        paragraphs
    }
}

pub(super) fn validate_paragraphs(paragraphs: &[String]) -> Result<()> {
    let text_bytes = paragraphs.iter().map(String::len).sum::<usize>();
    let separators = paragraphs.len().saturating_sub(1);
    if paragraphs.is_empty() || paragraphs.len() > 500 || text_bytes + separators > 10_000 {
        bail!("note contents are too long");
    }
    Ok(())
}

pub(super) fn validate_note_content(note: &WsNote) -> Result<()> {
    validate_title(&note.title)?;
    validate_paragraphs(&note.paragraphs)?;
    if note.text != note.paragraphs.join("\n") {
        bail!("note text projection is inconsistent");
    }
    Ok(())
}

pub(super) fn validate_linked_shell_ids(
    linked_shell_ids: &[Sid],
    _page_id: u32,
    shells: &[(Sid, WsWinsize)],
) -> Result<()> {
    let mut unique = HashSet::new();
    if linked_shell_ids.len() > 100
        || linked_shell_ids
            .iter()
            .any(|id| !unique.insert(*id) || !shells.iter().any(|(shell_id, _)| shell_id == id))
    {
        bail!("note references an invalid terminal");
    }
    Ok(())
}

pub(super) fn validate_linked_note_ids(
    linked_note_ids: &[Sid],
    source_id: Sid,
    _page_id: u32,
    notes: &[(Sid, WsNote)],
) -> Result<()> {
    let mut unique = HashSet::new();
    if linked_note_ids.len() > 100
        || linked_note_ids.iter().any(|id| {
            *id == source_id
                || !unique.insert(*id)
                || !notes.iter().any(|(note_id, _)| note_id == id)
        })
    {
        bail!("note references an invalid note");
    }
    Ok(())
}

pub(super) fn validate_linked_file_window_ids(
    linked_file_window_ids: &[Sid],
    _page_id: u32,
    windows: &[(Sid, WsFileWindow)],
) -> Result<()> {
    let mut unique = HashSet::new();
    if linked_file_window_ids.len() > 100
        || linked_file_window_ids
            .iter()
            .any(|id| !unique.insert(*id) || !windows.iter().any(|(window_id, _)| window_id == id))
    {
        bail!("note references an invalid file editor");
    }
    Ok(())
}

pub(super) fn normalize_note_canvas_links(
    notes: &mut [(Sid, WsNote)],
    windows: &[(Sid, WsFileWindow)],
) {
    let note_ids = notes.iter().map(|(id, _)| *id).collect::<HashSet<_>>();
    let window_ids = windows.iter().map(|(id, _)| *id).collect::<HashSet<_>>();
    for (source_id, note) in notes {
        let mut unique_notes = HashSet::new();
        note.linked_note_ids
            .retain(|id| *id != *source_id && unique_notes.insert(*id) && note_ids.contains(id));
        note.linked_note_ids.truncate(100);

        let mut unique_windows = HashSet::new();
        note.linked_file_window_ids
            .retain(|id| unique_windows.insert(*id) && window_ids.contains(id));
        note.linked_file_window_ids.truncate(100);
    }
}

pub(super) fn normalize_linked_shell_ids(
    linked_shell_ids: Vec<u32>,
    _page_id: u32,
    shells: &[(Sid, WsWinsize)],
) -> Vec<Sid> {
    let mut unique = HashSet::new();
    linked_shell_ids
        .into_iter()
        .map(Sid)
        .filter(|id| unique.insert(*id) && shells.iter().any(|(shell_id, _)| shell_id == id))
        .take(100)
        .collect()
}

pub(super) fn validate_color(color: &str) -> Result<()> {
    if !color.is_empty()
        && !(color.len() == 7
            && color.starts_with('#')
            && color[1..].bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        bail!("invalid background color");
    }
    Ok(())
}

pub(super) fn validate_opacity(opacity: u8) -> Result<()> {
    if !(20..=100).contains(&opacity) {
        bail!("opacity must be between 20 and 100");
    }
    Ok(())
}

pub(super) fn validate_page_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.len() > 100 {
        bail!("page name must contain between 1 and 100 bytes");
    }
    Ok(())
}

pub(super) fn validate_theme(theme: &str) -> Result<()> {
    if theme.len() > 100 || theme.chars().any(char::is_control) {
        bail!("terminal color theme is invalid");
    }
    Ok(())
}

pub(super) fn validate_terminal_window_size(width: u16, height: u16) -> Result<()> {
    if (width == 0) != (height == 0)
        || (width != 0 && !(240..=4_000).contains(&width))
        || (height != 0 && !(160..=4_000).contains(&height))
    {
        bail!("terminal window dimensions are out of range");
    }
    Ok(())
}

pub(super) fn validate_optional_ssh_profile_id(id: &str) -> Result<()> {
    if !id.is_empty()
        && (id.len() > 64
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')))
    {
        bail!("SSH connection ID is invalid");
    }
    Ok(())
}

pub(super) fn validate_ssh_profile(profile: &WsSshProfile, others: &[WsSshProfile]) -> Result<()> {
    if profile.id.is_empty()
        || profile.id.len() > 64
        || !profile
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        bail!("SSH connection ID is invalid");
    }
    if profile.name.trim().is_empty() || profile.name.len() > 100 {
        bail!("SSH connection name must contain between 1 and 100 bytes");
    }
    if others
        .iter()
        .any(|other| other.name.eq_ignore_ascii_case(profile.name.trim()))
    {
        bail!("SSH connection names must be unique");
    }
    if profile.host.trim().is_empty()
        || profile.host.len() > 255
        || profile.host.starts_with('-')
        || profile
            .host
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        bail!("SSH host is invalid");
    }
    if profile.port == 0 {
        bail!("SSH port must be positive");
    }
    if profile.username.len() > 100
        || profile.username.starts_with('-')
        || profile
            .username
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        bail!("SSH username is invalid");
    }
    if profile.key_path.len() > 4_096 || profile.key_path.contains('\0') {
        bail!("SSH private key path is invalid");
    }
    if profile.auth_method == WsSshAuthMethod::KeyFile && profile.key_path.trim().is_empty() {
        bail!("SSH private key path is required");
    }
    validate_theme(&profile.theme)?;
    if profile.background_enabled {
        validate_color(&profile.background)?;
        if profile.background.is_empty() {
            bail!("SSH background override requires a color");
        }
    }
    Ok(())
}

pub(super) fn ws_profile_from_proto(profile: SshProfile) -> Result<WsSshProfile> {
    let auth_method = match SshAuthMethod::try_from(profile.auth_method)
        .map_err(|_| anyhow::anyhow!("unsupported SSH authentication method"))?
    {
        SshAuthMethod::SshAuthDefault => WsSshAuthMethod::Default,
        SshAuthMethod::SshAuthAgent => WsSshAuthMethod::Agent,
        SshAuthMethod::SshAuthKeyFile => WsSshAuthMethod::KeyFile,
        SshAuthMethod::SshAuthPassword => WsSshAuthMethod::Password,
    };
    Ok(WsSshProfile {
        id: profile.id,
        name: profile.name,
        host: profile.host,
        port: profile
            .port
            .try_into()
            .context("SSH port is out of range")?,
        username: profile.username,
        auth_method,
        key_path: profile.key_path,
        accept_new_host_key: profile.accept_new_host_key,
        theme: profile.theme,
        background_enabled: profile.background_enabled,
        background: profile.background,
    })
}

pub(super) fn proto_profile_from_ws(profile: WsSshProfile) -> SshProfile {
    let auth_method = match profile.auth_method {
        WsSshAuthMethod::Default => SshAuthMethod::SshAuthDefault,
        WsSshAuthMethod::Agent => SshAuthMethod::SshAuthAgent,
        WsSshAuthMethod::KeyFile => SshAuthMethod::SshAuthKeyFile,
        WsSshAuthMethod::Password => SshAuthMethod::SshAuthPassword,
    };
    SshProfile {
        id: profile.id,
        name: profile.name,
        host: profile.host,
        port: profile.port.into(),
        username: profile.username,
        auth_method: auth_method.into(),
        key_path: profile.key_path,
        accept_new_host_key: profile.accept_new_host_key,
        theme: profile.theme,
        background_enabled: profile.background_enabled,
        background: profile.background,
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_custom_window, validate_terminal_window_size};
    use crate::web::protocol::WsCustomWindow;

    #[test]
    fn validates_terminal_geometry_boundaries() {
        assert!(validate_terminal_window_size(0, 0).is_ok());
        assert!(validate_terminal_window_size(240, 160).is_ok());
        assert!(validate_terminal_window_size(4_000, 4_000).is_ok());
        assert!(validate_terminal_window_size(239, 160).is_err());
        assert!(validate_terminal_window_size(240, 159).is_err());
        assert!(validate_terminal_window_size(0, 160).is_err());
    }

    #[test]
    fn validates_two_by_three_grid_custom_component_minimum() {
        let window = WsCustomWindow {
            page_id: 1,
            title: "Widget".into(),
            background: "#18181b".into(),
            x: 4,
            y: 4,
            width: 72,
            height: 112,
            source: "<p>Widget</p>".into(),
            show_preview: false,
            url: String::new(),
            use_url: false,
            minimized: false,
        };
        assert!(validate_custom_window(&window).is_ok());
        assert!(validate_custom_window(&WsCustomWindow {
            width: 71,
            ..window.clone()
        })
        .is_err());
        assert!(validate_custom_window(&WsCustomWindow {
            height: 111,
            ..window
        })
        .is_err());
    }

    #[test]
    fn validates_custom_component_url_mode() {
        let window = WsCustomWindow {
            page_id: 1,
            title: "Status".into(),
            background: "#18181b".into(),
            x: 4,
            y: 4,
            width: 320,
            height: 240,
            source: String::new(),
            show_preview: true,
            url: "https://status.example.test/dashboard".into(),
            use_url: true,
            minimized: false,
        };
        assert!(validate_custom_window(&window).is_ok());
        assert!(validate_custom_window(&WsCustomWindow {
            show_preview: false,
            url: "https://".into(),
            ..window.clone()
        })
        .is_ok());
        assert!(validate_custom_window(&WsCustomWindow {
            url: "javascript:alert(1)".into(),
            ..window
        })
        .is_err());
    }
}
