#[test]
fn bundled_macos_info_plist_declares_screen_and_folder_usage_descriptions() {
    let plist = include_str!("../Info.plist");
    for key in [
        "NSScreenCaptureUsageDescription",
        "NSDocumentsFolderUsageDescription",
        "NSDesktopFolderUsageDescription",
        "NSDownloadsFolderUsageDescription",
        "NSRemovableVolumesUsageDescription",
        "NSNetworkVolumesUsageDescription",
    ] {
        assert!(
            plist.contains(key),
            "src-tauri/Info.plist must declare {key} so macOS can persist TCC permissions"
        );
    }
}

#[test]
fn macos_entitlements_keep_app_unsandboxed_with_network_and_file_access() {
    let entitlements = include_str!("../Entitlements.plist");
    assert!(
        entitlements.contains("com.apple.security.app-sandbox"),
        "explicit sandbox key is required so the app is not accidentally confined"
    );
    assert!(
        entitlements.contains("<false/>"),
        "Coding Tools MCP requires an unsandboxed macOS runtime"
    );
    assert!(entitlements.contains("com.apple.security.files.user-selected.read-write"));
    assert!(entitlements.contains("com.apple.security.files.downloads.read-write"));
    assert!(entitlements.contains("com.apple.security.network.client"));
    assert!(entitlements.contains("com.apple.security.network.server"));
}
