use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Every .proto under proto/calcifer/v1, so a new file needs no edit here.
    let package = Path::new("../calcifer/v1");
    let mut protos = Vec::new();
    for entry in std::fs::read_dir(package)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "proto") {
            protos.push(path);
        }
    }
    protos.sort();
    println!("cargo:rerun-if-changed={}", package.display());

    // The descriptor set, for the server's gRPC reflection.
    let descriptor = PathBuf::from(std::env::var("OUT_DIR")?).join("calcifer_descriptor.bin");
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(false)
        .file_descriptor_set_path(descriptor)
        .compile_protos(&protos, &[PathBuf::from("..")])?;
    Ok(())
}
