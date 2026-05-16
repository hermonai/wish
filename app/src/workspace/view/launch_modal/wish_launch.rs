use super::{CTAButton, CheckboxConfig, LaunchModalEvent, Slide};
use crate::ai::ambient_agents::telemetry::{CloudAgentTelemetryEvent, CloudModeEntryPoint};
use crate::terminal::view::OnboardingIntention;
use crate::ui_components::icons::Icon;
use crate::workspace::action::WorkspaceAction;
use crate::workspace::view::OnboardingTutorial;
use crate::workspaces::user_workspaces::UserWorkspaces;
use crate::workspaces::workspace::{AdminEnablementSetting, UgcCollectionEnablementSetting};
use asset_macro::bundled_or_fetched_asset;
use markdown_parser::{FormattedTextFragment, FormattedTextLine};
use wish_core::send_telemetry_from_ctx;
use wishui::assets::asset_cache::AssetSource;
use wishui::{AppContext, SingletonEntity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HermonLaunchSlide {
    CloudAgents,
    AgentAutomations,
    AgentManagement,
    LaunchCredits,
}

impl Slide for HermonLaunchSlide {
    fn modal_title(&self) -> String {
        "Introducing Hermon".to_string()
    }

    fn modal_subtext_paragraphs(&self) -> Vec<FormattedTextLine> {
        vec![FormattedTextLine::Line(vec![
            FormattedTextFragment::plain_text(
                "Infinitely scalable coding agent — run in local sessions or in the cloud.",
            ),
        ])]
    }

    fn first() -> Self {
        HermonLaunchSlide::CloudAgents
    }

    fn next(&self) -> Option<Self> {
        match self {
            HermonLaunchSlide::CloudAgents => Some(HermonLaunchSlide::AgentAutomations),
            HermonLaunchSlide::AgentAutomations => Some(HermonLaunchSlide::AgentManagement),
            HermonLaunchSlide::AgentManagement => Some(HermonLaunchSlide::LaunchCredits),
            HermonLaunchSlide::LaunchCredits => None,
        }
    }

    fn prev(&self) -> Option<Self> {
        match self {
            HermonLaunchSlide::CloudAgents => None,
            HermonLaunchSlide::AgentAutomations => Some(HermonLaunchSlide::CloudAgents),
            HermonLaunchSlide::AgentManagement => Some(HermonLaunchSlide::AgentAutomations),
            HermonLaunchSlide::LaunchCredits => Some(HermonLaunchSlide::AgentManagement),
        }
    }

    fn display_text(&self) -> Option<&'static str> {
        Some(match self {
            HermonLaunchSlide::CloudAgents => "Cloud agents",
            HermonLaunchSlide::AgentAutomations => "Agent automations",
            HermonLaunchSlide::AgentManagement => "Agent management",
            HermonLaunchSlide::LaunchCredits => "A little gift",
        })
    }

    fn short_label(&self) -> &'static str {
        match self {
            HermonLaunchSlide::CloudAgents => "Cloud agents",
            HermonLaunchSlide::AgentAutomations => "Agent automations",
            HermonLaunchSlide::AgentManagement => "Agent management",
            HermonLaunchSlide::LaunchCredits => "Launch credits",
        }
    }

    fn title(&self) -> &'static str {
        match self {
            HermonLaunchSlide::CloudAgents => "Break out of your laptop with cloud agents",
            HermonLaunchSlide::AgentAutomations => {
                "Orchestrate agents, turning Skills into automations"
            }
            HermonLaunchSlide::AgentManagement => "Track local and cloud agents seamlessly",
            HermonLaunchSlide::LaunchCredits => {
                "1,000 free cloud agent credits when you upgrade to Wish Build"
            }
        }
    }

    fn title_icon(&self) -> Option<Icon> {
        None
    }

    fn content(&self) -> &'static str {
        match self {
            HermonLaunchSlide::CloudAgents => {
                "Use cloud agents to run many agents in parallel, keep agents working when you close your laptop, or start agents programmatically. Plus, you can check on their work through the web."
            }
            HermonLaunchSlide::AgentAutomations => {
                "Hermon agents can be defined using the standard Skills format. You can use the built in scheduler to setup agents to run autonomously at set intervals, or use the Wish SDK or API to programmatically start and manage Hermon agents."
            }
            HermonLaunchSlide::AgentManagement => {
                "View all of your agents across local and cloud sessions in the Wish app or at [wish.hermon.ai](https://wish.hermon.ai). Join live agent sessions, continue tasks locally, and steer agents with one click."
            }
            HermonLaunchSlide::LaunchCredits => {
                "Upgrade to Build this month and receive 1,000 extra credits to try using Hermon Agent. Credits are only eligible for Hermon Cloud runs in Wish-hosted cloud environments."
            }
        }
    }

    fn image(&self) -> AssetSource {
        // TODO: Replace with new images once provided.
        match self {
            HermonLaunchSlide::CloudAgents => {
                bundled_or_fetched_asset!("png/hermon_cloud_agents.png")
            }
            HermonLaunchSlide::AgentAutomations => {
                bundled_or_fetched_asset!("png/hermon_agent_automations.png")
            }
            HermonLaunchSlide::AgentManagement => {
                bundled_or_fetched_asset!("png/hermon_agent_management.png")
            }
            HermonLaunchSlide::LaunchCredits => {
                bundled_or_fetched_asset!("png/hermon_launch_credits.png")
            }
        }
    }

    fn all() -> Vec<Self> {
        vec![
            HermonLaunchSlide::CloudAgents,
            HermonLaunchSlide::AgentAutomations,
            HermonLaunchSlide::AgentManagement,
            HermonLaunchSlide::LaunchCredits,
        ]
    }

    fn cta_button(&self) -> CTAButton<Self> {
        match self {
            HermonLaunchSlide::CloudAgents
            | HermonLaunchSlide::AgentAutomations
            | HermonLaunchSlide::AgentManagement => {
                let next = self.next().expect("Non-final slides should have a next");
                CTAButton::next_slide(next, format!("Next: {}", next.short_label()))
            }
            HermonLaunchSlide::LaunchCredits => CTAButton::custom("Try it out", |ctx| {
                send_telemetry_from_ctx!(
                    CloudAgentTelemetryEvent::EnteredCloudMode {
                        entry_point: CloudModeEntryPoint::HermonLaunchModal,
                    },
                    ctx
                );
                ctx.emit(LaunchModalEvent::Close);
                ctx.dispatch_typed_action(&WorkspaceAction::StartAgentOnboardingTutorial(
                    OnboardingTutorial::NoProject {
                        intention: OnboardingIntention::AgentDrivenDevelopment,
                    },
                ));
                ctx.dispatch_typed_action(&WorkspaceAction::AddAmbientAgentTab);
            }),
        }
    }

    fn secondary_cta_button(&self) -> Option<CTAButton<Self>> {
        match self {
            HermonLaunchSlide::LaunchCredits => Some(CTAButton::close("Skip for now")),
            HermonLaunchSlide::CloudAgents
            | HermonLaunchSlide::AgentAutomations
            | HermonLaunchSlide::AgentManagement => None,
        }
    }

    fn checkbox_config(&self) -> Option<CheckboxConfig> {
        Some(CheckboxConfig {
            label: "Sync conversations to cloud",
            description: "Agent conversations stored in the cloud can be shared with anyone with one click, and allow conversations to be continued across devices and on logout.",
        })
    }

    fn should_show_checkbox(&self, app: &AppContext) -> bool {
        let cloud_storage_setting =
            UserWorkspaces::as_ref(app).get_cloud_conversation_storage_enablement_setting();
        let ugc_setting = UserWorkspaces::as_ref(app).get_ugc_collection_enablement_setting();

        // Show checkbox only when user has control over cloud storage AND UGC is not force-enabled.
        matches!(
            cloud_storage_setting,
            AdminEnablementSetting::RespectUserSetting
        ) && !matches!(ugc_setting, UgcCollectionEnablementSetting::Enable)
    }

    fn on_close(&self, ctx: &mut wishui::ViewContext<super::LaunchModal<Self>>) {
        ctx.dispatch_typed_action(&WorkspaceAction::StartAgentOnboardingTutorial(
            OnboardingTutorial::NoProject {
                intention: OnboardingIntention::AgentDrivenDevelopment,
            },
        ));
    }
}

pub fn init(app: &mut wishui::AppContext) {
    super::init::<HermonLaunchSlide>(app);
}
