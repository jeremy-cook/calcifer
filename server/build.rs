fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile_protos(
            &[
                "../proto/calcifer/v1/entities.proto",
                "../proto/calcifer/v1/services.proto",
                "../proto/calcifer/v1/structures.proto",
            ],
            &["../proto"],
        )?;
    Ok(())
}
