fn main() {
    println!("cargo:rerun-if-env-changed=DRAWDOCK_VERSION");
    println!("cargo:rerun-if-changed=assets/drawdock.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = std::env::var("DRAWDOCK_VERSION")
        .unwrap_or_else(|_| std::env::var("CARGO_PKG_VERSION").unwrap());
    let parts: Vec<u16> = version
        .split('.')
        .map(|part| {
            part.parse()
                .expect("DRAWDOCK_VERSION must be major.minor.patch")
        })
        .collect();
    assert_eq!(parts.len(), 3, "DRAWDOCK_VERSION must be major.minor.patch");
    let packed =
        (u64::from(parts[0]) << 48) | (u64::from(parts[1]) << 32) | (u64::from(parts[2]) << 16);
    winresource::WindowsResource::new()
        .set_icon("assets/drawdock.ico")
        .set("CompanyName", "Mike Cerreto")
        .set("ProductName", "DrawDock")
        .set("FileDescription", "DrawDock Windows Host and Launcher")
        .set("FileVersion", &version)
        .set("ProductVersion", &version)
        .set_version_info(winresource::VersionInfo::FILEVERSION, packed)
        .set_version_info(winresource::VersionInfo::PRODUCTVERSION, packed)
        .compile()
        .expect("compile DrawDock Windows version and icon resources");
}
