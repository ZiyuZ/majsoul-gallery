fn main() {
    println!("cargo:rerun-if-changed=src/assets/app.rc");
    println!("cargo:rerun-if-changed=src/assets/app-icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("src/assets/app.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("无法嵌入 Windows 应用图标");
    }
}
