//! Native application shell for the shared sshxx client frontend.

/// Starts the cross-platform sshxx client application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("failed to run sshxx-client application");
}
