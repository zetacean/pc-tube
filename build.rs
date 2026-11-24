fn main() {
    // Embeds assets/icon.ico in the .exe (Explorer, taskbar, Alt+Tab).
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    embed_resource::compile("app.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
