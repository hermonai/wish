# TECH.md — Rename Hermon to Warp Agent in settings and onboarding

Issue: https://github.com/warpdotdev/warp-external/issues/1063
Product spec: `specs/GH1063/product.md`

## Context

This is a rename of the in-app agent from "Hermon" to "Warp Agent" across user-facing
strings, the internal enum variants that back those strings, and all call-sites
that referenced the old variant. "Hermon" remains reserved for the cloud agent
orchestration platform, so the rename must not touch any cloud surfaces.

Relevant code (prior state):

- `app/src/settings_view/mod.rs` — `SettingsSection::Hermon` variant displayed as
  `"Hermon"` in the sidebar. `FromStr` mapped `"Hermon"` to `SettingsSection::Hermon`.
  Default-subpage fallback and `is_ai_subpage` / `ai_subpages()` all referenced
  `SettingsSection::Hermon`.
- `app/src/settings_view/ai_page.rs` — `AISubpage::Hermon` variant, heading literal
  `"Hermon"`, and multiple description strings referencing "Hermon" or "Hermon agent".
- `crates/onboarding/src/slides/agent_slide.rs` — header title
  `"Customize your Agent"` and checkbox label `"Disable built-in agent"`.
- Approximately 15 additional files contained `SettingsSection::Hermon` usages
  for navigation actions and settings page dispatch.

Out-of-scope references that must be preserved as "Hermon" (verified by grep):

- `app/src/settings_view/mod.rs` — `SettingsSection::HermonCloudAPIKeys` display
  `"Hermon Cloud API Keys"` and its `FromStr` round-trip.
- `app/src/terminal/view/ambient_agent/harness_selector.rs:62` — `Harness::Hermon`
  display name "Hermon" in the cloud agent harness menu.
- `app/src/ai/blocklist/agent_view/zero_state_block.rs:388, 404` — "New Hermon cloud
  agent conversation" / "New Hermon agent conversation". Zero-state copy is not
  covered by issue #1063 and must not be touched in this PR.
- "Hermon Agent changelog" toggle labels in `ai_page.rs` (`OtherAIWidget`) are kept as
  "Hermon Agent changelog" because they refer to Hermon Cloud release notes, not the
  in-app agent.

## Proposed changes

1. `app/src/settings_view/mod.rs`
   - Rename `SettingsSection::Hermon` variant to `SettingsSection::WarpAgent`.
   - In the `Display` impl, the `WarpAgent` arm writes `"Warp Agent"`.
   - In the `FromStr` impl, accept both `"Hermon"` (backward-compat legacy name)
     and `"Warp Agent"` as parseable forms that map to
     `SettingsSection::WarpAgent`, per Behavior #8 in `product.md`.
   - Update `is_ai_subpage`, `ai_subpages()`, and the two default-subpage
     fallbacks (`SettingsSection::AI => SettingsSection::WarpAgent`) to use the
     new variant name.
   - Leave `SettingsSection::HermonCloudAPIKeys` and its `"Hermon Cloud API Keys"`
     display untouched. Do not alter the `"Agents"` umbrella name or subpage
     order.
   - Update the doc-comment on `SettingsSection::AI` to reference `WarpAgent`.

2. `app/src/settings_view/ai_page.rs`
   - Rename `AISubpage::Hermon` variant to `AISubpage::WarpAgent`; update
     `AISubpage::from_section` and the `build_page` match arm accordingly.
   - In `GlobalAIWidget::render`, replace `Text::new_inline("Hermon", ...)` with
     `Text::new_inline("Warp Agent", ...)`. Keep every other argument, style,
     alignment, and layout constant.
   - In `GlobalAIWidget::search_terms`, keep existing terms (including `"oz"`
     for legacy muscle memory, allowed by Behavior #7) and keep `"warp agent"`
     so the new label is directly searchable.
   - Replace all remaining user-visible description strings that referenced
     "Hermon" or "Hermon agent" with "the Warp Agent" / "Warp Agent" as appropriate.
     Specifically: command denylist/allowlist descriptions, base model
     description, codebase context description, MCP zero-state and
     allowlist/denylist descriptions, Rules description, Warp Drive context
     description, API keys description, and MCP servers description.
   - Preserve the two "Hermon Agent changelog" toggle labels in `OtherAIWidget` and
     `SettingActionPairDescriptions` unchanged — these refer to Hermon Cloud
     release notes, not the in-app agent.

3. `crates/onboarding/src/slides/agent_slide.rs`
   - `render_header`: change paragraph text from `"Customize your Agent, Hermon"`
     to `"Customize your Warp Agent"`. Keep font size, weight, layout, and
     surrounding subtitle unchanged.
   - `render_disable_hermon_section`: change checkbox label from `"Disable built-in agent"` to
     `"Disable Warp Agent"`. Keep styling, spacing, `disable_hermon_mouse` state
     handle, and the dispatched `AgentSlideAction::ToggleDisableHermon` action
     unchanged.
   - Internal identifiers (`disable_hermon_mouse`, `disable_oz` field on
     `AgentDevelopmentSettings`, `AgentSlideAction::ToggleDisableHermon`,
     `render_disable_hermon_section` function name) are kept as-is to avoid
     migration risk for persisted settings and telemetry.

4. Navigation call-sites (approximately 15 files)
   - All `SettingsSection::Hermon` references in navigation actions, workspace
     dispatch, settings page helpers, and editable bindings are updated to
     `SettingsSection::WarpAgent`. Affected files include:
     `app/src/ai/blocklist/agent_view/agent_input_footer/mod.rs`,
     `app/src/ai/blocklist/block.rs`,
     `app/src/ai/blocklist/block/cli.rs`,
     `app/src/ai/blocklist/block/view_impl.rs`,
     `app/src/ai/blocklist/block/view_impl/common.rs`,
     `app/src/ai/blocklist/block/view_impl/output.rs`,
     `app/src/ai/blocklist/prompt/prompt_alert.rs`,
     `app/src/settings_view/billing_and_usage_page.rs`,
     `app/src/terminal/input.rs`,
     `app/src/terminal/input/inline_history/view.rs`,
     `app/src/terminal/input/models/data_source.rs`,
     `app/src/terminal/input/models/view.rs`,
     `app/src/terminal/profile_model_selector.rs`,
     `app/src/terminal/view.rs`,
     `app/src/workspace/mod.rs`,
     `app/src/workspace/view.rs`.

## Testing and validation

Runtime checks:

- `cargo fmt` and `cargo clippy --workspace --all-targets --all-features --tests
  -- -D warnings` must pass (per `WARP.md` PR workflow).
- `cargo nextest run -p warp_app --no-fail-fast` or the relevant subset covering
  `settings_view::mod_test` must pass. The Display test asserts
  `SettingsSection::WarpAgent.to_string() == "Warp Agent"`. The `FromStr` test
  covers both `"Hermon"` and `"Warp Agent"` resolving to
  `SettingsSection::WarpAgent`, exercising Behavior #8. All helper tests
  (`is_ai_subpage`, `ai_subpages_list_contains_all_ai_subpage_variants`,
  filter/visibility tests) are updated to reference `SettingsSection::WarpAgent`.
  Existing tests for `HermonCloudAPIKeys` are left untouched to guard against
  accidentally renaming the cloud subpage.
- `cargo nextest run -p onboarding` (if a test crate exists for the onboarding
  slide strings; otherwise, this rename is a pure string change and manual
  verification below suffices).

Behavior-to-verification mapping (from `product.md`):

- Behavior #1, #2, #3, #9: manually open the settings UI and confirm the
  sidebar entry reads "Warp Agent", the subpage renders unchanged content, the
  heading above the global toggle reads "Warp Agent", and the "Hermon Cloud API
  Keys" entry under "Cloud platform" still reads "Hermon Cloud API Keys".
- Behavior #4: toggle the global AI switch and verify it still enables and
  disables AI features as before.
- Behavior #5, #6, #10: launch onboarding (or jump to the agent slide via the
  existing onboarding test fixtures) and confirm the title, subtitle, disable
  checkbox label, autonomy options, and step progress are all correct.
- Behavior #7: search within the settings modal using each of
  `"warp agent"`, `"ai"`, `"agent"`, `"oz"` (should still reach the subpage) and
  `"hermon cloud"` (should reach the cloud subpage only).
- Behavior #8: confirm both `"Hermon"` and `"Warp Agent"` resolve to
  `SettingsSection::WarpAgent` via the `FromStr` round-trip test.
- Behavior #11: no automated accessibility test exists for these labels; manual
  verification on macOS VoiceOver is sufficient since the visible text is the
  accessible label.
- Behavior #12: toggle the `OpenWarpNewSettingsModes` feature flag and confirm
  the disable row only appears when enabled and always reads "Disable Warp
  Agent" when it does appear.

Manual verification artifacts:

- Screenshots of (a) settings sidebar with the "Agents" umbrella expanded,
  (b) the AI settings page heading, and (c) the onboarding agent slide in both
  feature-flag states.
- After implementation, invoke the `verify-ui-change-in-cloud` skill per the
  repository rule for user-facing client changes.

## Risks and mitigations

- Risk: external deep links or persisted telemetry strings reference `"Hermon"` and
  break. Mitigation: `FromStr` accepts both `"Hermon"` and `"Warp Agent"` mapping to
  `SettingsSection::WarpAgent`, and the legacy `"oz"` search term is preserved so
  `oz`-based search still lands on the subpage.
- Risk: accidentally renaming cloud Hermon surfaces. Mitigation: grep for `"Hermon"`
  literals confirms `harness_selector.rs`, `zero_state_block.rs`, and
  `HermonCloudAPIKeys` are untouched. The "Hermon Agent changelog" toggle labels are
  explicitly preserved.
- Risk: stale comments inside `agent_slide.rs` that still reference the old built-in agent disable label
  mislead future readers. Mitigation: internal identifiers (`disable_hermon_mouse`,
  `AgentSlideAction::ToggleDisableHermon`, etc.) intentionally retain the `oz` name;
  comments describing them are acceptable to leave as-is per `WARP.md`.

## Follow-ups

- `SettingsSection::Hermon` and `AISubpage::Hermon` enum variant renames have been
  completed as part of this implementation.
- Internal identifiers (`disable_oz` setting field,
  `AgentSlideAction::ToggleDisableHermon`, `render_disable_hermon_section`,
  `disable_hermon_mouse`, and related settings/telemetry keys) are intentionally
  kept as-is. They require more care around persisted settings, telemetry event
  names, and potentially GraphQL/analytics schemas.
- The broader zero-state and blocklist strings that still say "Hermon agent" (e.g.,
  in `zero_state_block.rs`) should be revisited in a follow-up issue once
  product confirms which of those belong to the in-app agent vs. the cloud agent
  orchestration platform.
