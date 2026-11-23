fn main() {
    #[cfg(target_os = "macos")]
    {
        cc::Build::new()
            .file("src/platform/macos/computer_native.m")
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .flag("-Wno-unguarded-availability-new")
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
        if let Ok(output) = std::process::Command::new("clang")
            .arg("--print-resource-dir")
            .output()
        {
            if output.status.success() {
                let dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let darwin_lib = std::path::Path::new(&dir).join("lib").join("darwin");
                if darwin_lib.join("libclang_rt.osx.a").exists() {
                    println!("cargo:rustc-link-search=native={}", darwin_lib.display());
                    println!("cargo:rustc-link-lib=static=clang_rt.osx");
                }
            }
        }
        println!("cargo:rerun-if-changed=src/platform/macos/computer_native.m");
    }

    #[cfg(feature = "desktop-ui")]
    tauri_build::build();
}
