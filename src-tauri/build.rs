fn main() {
    #[cfg(target_os = "macos")]
    {
        cc::Build::new()
            .file("src/platform/macos/computer_native.m")
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .compile("computer_native");
        for framework in [
            "Foundation",
            "AppKit",
            "CoreGraphics",
            "ScreenCaptureKit",
            "ApplicationServices",
            "Security",
        ] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
        println!("cargo:rerun-if-changed=src/platform/macos/computer_native.m");
    }
    tauri_build::build()
}
