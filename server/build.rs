use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Every .proto under ../proto/calcifer/v1, so a new file needs no edit here.
    let package = Path::new("../proto/calcifer/v1");
    let mut protos = Vec::new();
    for entry in std::fs::read_dir(package)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "proto") {
            protos.push(path);
        }
    }
    protos.sort();
    println!("cargo:rerun-if-changed={}", package.display());

    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile_protos(&protos, &["../proto"])?;
    Ok(())
}
