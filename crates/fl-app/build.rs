// Embeds the app icon in forkline.exe. GPUI loads the window and taskbar icon
// from resource id 1 (`load_icon` in gpui-pre-windows), which `set_icon` uses.
fn main() {
    println!("cargo:rerun-if-changed=resources/forkline.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("resources/forkline.ico")
            .compile()
            .expect("embed the app icon");
    }
}
