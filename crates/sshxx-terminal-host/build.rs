fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/terminal_host.proto");
    prost_build::compile_protos(&["proto/terminal_host.proto"], &["proto"])?;
    Ok(())
}
