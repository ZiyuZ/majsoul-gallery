use protox::prost::Message;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=protocol/liqi.proto");
    let descriptors = protox::compile(["protocol/liqi.proto"], ["protocol"])?;
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(output.join("liqi.bin"), descriptors.encode_to_vec())?;
    Ok(())
}
