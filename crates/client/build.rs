fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Gives the .exe its icon in the file explorer.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=../../public/icon.ico");
        winresource::WindowsResource::new()
            .set_icon("../../public/icon.ico")
            .compile()
            .expect("failed to embed the Windows icon");
    }
}
