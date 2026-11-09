//! Embeds the application icon and the version information into the Windows executable, so
//! Explorer, shortcuts and the file properties show them. The window icon is set at runtime.

const ICON: &str = "../../assets/icons/app_icon.ico";

fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed={ICON}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    winresource::WindowsResource::new()
        .set_icon(ICON)
        .set("ProductName", "FileCraft")
        .set("FileDescription", "FileCraft")
        .compile()
}
