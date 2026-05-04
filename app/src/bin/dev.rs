// On Windows, we don't want to display a console window when the application is running in release
// builds. See https://doc.rust-lang.org/reference/runtime.html#the-windows_subsystem-attribute.
#![cfg_attr(feature = "release_bundle", windows_subsystem = "windows")]

use anyhow::Result;
use wish_core::{
    channel::{Channel, ChannelConfig, ChannelState, HermonConfig, WishServerConfig},
    features, AppId,
};

// The `wish-dev` binary uses inline config — no external config generator needed.
// Dev channel with debug + dogfood + preview features enabled.
fn main() -> Result<()> {
    ChannelState::set(
        ChannelState::new(
            Channel::Dev,
            ChannelConfig {
                app_id: AppId::new("ai", "hermon", "WishDev"),
                logfile_name: "wish-dev.log".into(),
                server_config: WishServerConfig::local_dev(),
                hermon_config: HermonConfig::local_dev(),
                telemetry_config: None,
                crash_reporting_config: None,
                autoupdate_config: None,
                mcp_static_config: None,
            },
        )
        .with_additional_features(features::DEBUG_FLAGS)
        .with_additional_features(features::DOGFOOD_FLAGS)
        .with_additional_features(features::PREVIEW_FLAGS),
    );

    wish::run()
}
