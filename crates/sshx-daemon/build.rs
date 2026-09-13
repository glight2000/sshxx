use std::{env, fs, path::PathBuf};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let package = root.join("node_modules/@xterm/headless/package.json");
    let library = root.join("node_modules/@xterm/headless/lib-headless/xterm-headless.js");
    let adapter = root.join("src/lib/terminalCheckpoint/compat.mjs");
    let archive = root.join("src/lib/terminalCheckpoint/archive.mjs");
    for path in [&package, &library, &adapter, &archive] {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let metadata = fs::read_to_string(package)
        .expect("Run npm ci at the repository root before building sshxx-daemon");
    assert!(
        metadata.contains("\"version\": \"6.0.0\""),
        "The terminal checkpoint adapter requires @xterm/headless 6.0.0"
    );
    // Embed the pinned npm artifact at compile time. Users need no Node runtime,
    // network download, separate worker process, or writable executable cache.
    let source = format!(
        "var exports = {{}}; var module = {{exports}};\n{}\n{}\n{}",
        fs::read_to_string(library).unwrap(),
        fs::read_to_string(adapter).unwrap().replace("export ", ""),
        embedded_module(&fs::read_to_string(archive).unwrap())
    );
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("terminal.js"),
        source,
    )
    .unwrap();
}

fn embedded_module(source: &str) -> String {
    let mut importing = false;
    source
        .lines()
        .filter(|line| {
            if line.starts_with("import ") {
                importing = true;
            }
            let keep = !importing;
            if importing && line.trim_end().ends_with(';') {
                importing = false;
            }
            keep
        })
        .collect::<Vec<_>>()
        .join("\n")
        .replace("export ", "")
}
