// On Windows, we don't want to display a console window when the application is running in release
// builds. See https://doc.rust-lang.org/reference/runtime.html#the-windows_subsystem-attribute.
#![cfg_attr(feature = "release_bundle", windows_subsystem = "windows")]

use anyhow::Result;
use wish_core::{
    channel::{Channel, ChannelConfig, ChannelState, HermonConfig, WishServerConfig},
    AppId,
};

// The `wish-stable` binary uses inline config — no external config generator needed.
// Stable channel: production defaults, no extra feature flags.
fn main() -> Result<()> {
    ChannelState::set(ChannelState::new(
        Channel::Stable,
        ChannelConfig {
            app_id: AppId::new("ai", "hermon", "Wish"),
            logfile_name: "wish.log".into(),
            server_config: WishServerConfig::production(),
            hermon_config: HermonConfig::production(),
            telemetry_config: None,
            crash_reporting_config: None,
            autoupdate_config: None,
            mcp_static_config: None,
        },
    ));

    wish::run()
}
