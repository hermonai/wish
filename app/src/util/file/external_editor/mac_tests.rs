use super::is_wish_bundle;

#[test]
fn is_wish_bundle_recognises_wish_channels() {
    assert!(is_wish_bundle("dev.warp.Warp"));
    assert!(is_wish_bundle("dev.warp.WarpDev"));
    assert!(is_wish_bundle("dev.warp.WarpPreview"));
    assert!(is_wish_bundle("dev.warp.WishOss"));
}

#[test]
fn is_wish_bundle_rejects_other_apps() {
    assert!(!is_wish_bundle("com.microsoft.VSCode"));
    assert!(!is_wish_bundle("com.apple.TextEdit"));
    assert!(!is_wish_bundle("dev.zed.Zed"));
    assert!(!is_wish_bundle("invalid"));
    assert!(!is_wish_bundle(""));
}
