use super::{
    settings_page::{
        MatchData, PageType, SettingsPageEvent, SettingsPageMeta, SettingsPageViewHandle,
        SettingsWidget,
    },
    SettingsSection,
};
use crate::{appearance::Appearance, channel::ChannelState, workspace::WorkspaceAction};
use wishui::{
    assets::asset_cache::AssetSource,
    elements::{
        Align, CacheOption, ConstrainedBox, Container, CrossAxisAlignment, Element, Flex, Image,
        MainAxisAlignment, MouseStateHandle, ParentElement, Wrap,
    },
    ui_components::{
        button::ButtonVariant,
        components::{UiComponent, UiComponentStyles},
    },
    AppContext, Entity, View, ViewContext, ViewHandle,
};

pub struct AboutPageView {
    page: PageType<Self>,
}

impl AboutPageView {
    pub fn new(_ctx: &mut ViewContext<AboutPageView>) -> Self {
        AboutPageView {
            page: PageType::new_monolith(AboutPageWidget::default(), None, false),
        }
    }
}

impl Entity for AboutPageView {
    type Event = SettingsPageEvent;
}

impl View for AboutPageView {
    fn ui_name() -> &'static str {
        "AboutPage"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

#[derive(Default)]
struct AboutPageWidget {
    copy_version_button_mouse_state: MouseStateHandle,
    welcome_button_mouse_state: MouseStateHandle,
    onboarding_button_mouse_state: MouseStateHandle,
}

impl SettingsWidget for AboutPageWidget {
    type View = AboutPageView;

    fn search_terms(&self) -> &str {
        "about wish version"
    }

    fn render(
        &self,
        _view: &AboutPageView,
        appearance: &Appearance,
        _app: &AppContext,
    ) -> Box<dyn Element> {
        let ui_builder = appearance.ui_builder();

        let version = ChannelState::app_version().unwrap_or(env!("CARGO_PKG_VERSION"));

        let version_text = ui_builder
            .span(version.to_string())
            .with_soft_wrap()
            .build()
            .with_margin_top(16.)
            .finish();

        let copy_version_icon = appearance
            .ui_builder()
            .copy_button(16., self.copy_version_button_mouse_state.clone())
            .build()
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(WorkspaceAction::CopyVersion(version));
            })
            .finish();

        let version_row = Wrap::row()
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_children([
                version_text,
                Container::new(copy_version_icon)
                    .with_margin_top(16.)
                    .with_padding_left(6.)
                    .finish(),
            ]);

        Align::new(
            Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(
                    Container::new(
                        ConstrainedBox::new(
                            Image::new(
                                AssetSource::Bundled {
                                    path: "bundled/svg/hermon-logo-round.svg",
                                },
                                CacheOption::BySize,
                            )
                            .finish(),
                        )
                        .with_height(80.)
                        .with_width(80.)
                        .finish(),
                    )
                    .with_margin_bottom(18.)
                    .finish(),
                )
                .with_child(
                    ui_builder
                        .paragraph("Wish")
                        .with_style(UiComponentStyles {
                            font_size: Some(32.),
                            ..Default::default()
                        })
                        .build()
                        .finish(),
                )
                .with_child(
                    Container::new(
                        ui_builder
                            .span("by Hermon AI")
                            .with_style(UiComponentStyles {
                                font_size: Some(14.),
                                ..Default::default()
                            })
                            .build()
                            .with_margin_top(4.)
                            .finish(),
                    )
                    .finish(),
                )
                .with_child(version_row.finish())
                .with_child(
                    ui_builder
                        .span("Copyright 2026 Hermon AI")
                        .build()
                        .with_margin_top(16.)
                        .finish(),
                )
                .with_child(
                    Container::new(
                        ui_builder
                            .button(
                                ButtonVariant::Secondary,
                                self.welcome_button_mouse_state.clone(),
                            )
                            .with_text_label("Show Welcome Page".to_string())
                            .build()
                            .on_click(|ctx, _, _| {
                                ctx.dispatch_typed_action(WorkspaceAction::ShowWelcomePage);
                            })
                            .finish(),
                    )
                    .with_margin_top(24.)
                    .finish(),
                )
                .with_child(
                    Container::new(
                        ui_builder
                            .button(
                                ButtonVariant::Secondary,
                                self.onboarding_button_mouse_state.clone(),
                            )
                            .with_text_label("Show Onboarding".to_string())
                            .build()
                            .on_click(|ctx, _, _| {
                                ctx.dispatch_typed_action(WorkspaceAction::ShowHoaOnboardingFlow);
                            })
                            .finish(),
                    )
                    .with_margin_top(12.)
                    .finish(),
                )
                .finish(),
        )
        .finish()
    }
}

impl SettingsPageMeta for AboutPageView {
    fn section() -> SettingsSection {
        SettingsSection::About
    }

    fn should_render(&self, _ctx: &AppContext) -> bool {
        true
    }

    fn update_filter(&mut self, query: &str, ctx: &mut ViewContext<Self>) -> MatchData {
        self.page.update_filter(query, ctx)
    }

    fn scroll_to_widget(&mut self, widget_id: &'static str) {
        self.page.scroll_to_widget(widget_id)
    }

    fn clear_highlighted_widget(&mut self) {
        self.page.clear_highlighted_widget();
    }
}

impl From<ViewHandle<AboutPageView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<AboutPageView>) -> Self {
        SettingsPageViewHandle::About(view_handle)
    }
}
