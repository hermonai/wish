// On Windows, we don't want to display a console window when the application is running in release
// builds. See https://doc.rust-lang.org/reference/runtime.html#the-windows_subsystem-attribute.
#![cfg_attr(feature = "release_bundle", windows_subsystem = "windows")]

use anyhow::Result;
use wish_core::{
    channel::{Channel, ChannelConfig, ChannelState, HermonConfig, WishServerConfig},
    features, AppId,
};

// The `wish` (local) binary uses inline config — no external config generator needed.
// This is the development channel with all debug + dogfood + preview features enabled.
fn main() -> Result<()> {
    // The `wish` binary now defaults to the production Hermon backend so that a
    // freshly-installed copy can sign up / sign in / sign out against
    // https://api.hermon.ai without any extra flags. Developers running a local
    // gateway can still override via `--server-root-url http://localhost:8080`
    // or `HERMON_API_URL=http://localhost:8080` since `Channel::Local` honors
    // those overrides (see `Channel::allows_server_url_overrides`).
    let mut state = ChannelState::new(
        Channel::Local,
        ChannelConfig {
            app_id: AppId::new("ai", "hermon", "Wish"),
            logfile_name: "wish-local.log".into(),
            server_config: WishServerConfig::production(),
            hermon_config: HermonConfig::production(),
            telemetry_config: None,
            crash_reporting_config: None,
            autoupdate_config: None,
            mcp_static_config: None,
        },
    )
    .with_additional_features(features::DEBUG_FLAGS)
    .with_additional_features(features::DOGFOOD_FLAGS)
    .with_additional_features(features::PREVIEW_FLAGS);

    // Enable sandbox telemetry feature flag if the env var is set.
    if std::env::var("WITH_SANDBOX_TELEMETRY").is_ok() {
        state = state.with_additional_features(&[features::FeatureFlag::WithSandboxTelemetry]);
    }

    ChannelState::set(state);

    wish::run()
}

// If we're not using an external plist, embed the following as the Info.plist.
#[cfg(all(not(feature = "extern_plist"), target_os = "macos"))]
embed_plist::embed_info_plist_bytes!(r#"
    <?xml version="1.0" encoding="UTF-8"?>
    <!DOCTYPE plist PUBLIC "-//Apple Computer//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
    <plist version="1.0">
    <dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleDisplayName</key>
    <string>Wish</string>
    <key>CFBundleExecutable</key>
    <string>wish</string>
    <key>CFBundleIdentifier</key>
    <string>ai.hermon.Wish</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Wish</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.developer-tools</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>UIDesignRequiresCompatibility</key>
    <true/>
    <key>CFBundleURLTypes</key>
    <array><dict><key>CFBundleURLName</key><string>Custom App</string><key>CFBundleURLSchemes</key><array><string>wish</string></array></dict></array>
    <key>NSHumanReadableCopyright</key>
    <string>Copyright (c) Hermon AI. All rights reserved.</string>
    </dict>
    </plist>
"#.as_bytes());
