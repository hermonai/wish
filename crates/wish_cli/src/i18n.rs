//! `wish i18n` — language-pack tooling driven by the Hermon gateway.
//!
//! Subcommands:
//!
//!   wish i18n locales
//!       Lists every locale the gateway knows about and whether it has
//!       an installed pack.
//!
//!   wish i18n get <locale>
//!       Prints the installed pack for `<locale>` as JSON to stdout.
//!       Useful for piping into a file:
//!         wish i18n get fr > fr.json
//!
//!   wish i18n install <pack.json>
//!       Uploads a `{ locale, label, strings }` JSON pack to the
//!       gateway, replacing any existing rows for that locale
//!       atomically.
//!
//!   wish i18n uninstall <locale>
//!       Removes the locale's pack. `en` is protected server-side.
//!
//!   wish i18n translate --target <locale> [--label <name>] [--style ...]
//!                       [--source <locale>] [--context <hint>]
//!                       [--model <id>] [--install] [--out <file>]
//!       The LLM-backed turnkey path. Asks the gateway's configured
//!       model to translate the source pack (default: installed `en`)
//!       into `--target`. With `--install`, the result is upserted into
//!       the gateway's database in the same transaction. With `--out`,
//!       the JSON is also written to disk so you can commit it.
//!
//! Auth: every subcommand uses the bearer credential stored by
//! `wish login --email …`. Translate + install/uninstall require an
//! admin allow-listed account; locales + get work for any signed-in
//! user.

use clap::{Args, Subcommand};

#[derive(Debug, Clone, Subcommand)]
pub enum I18nCommand {
    /// List every curated locale and its install status.
    Locales,
    /// Print the installed pack for a locale (English-fallback if absent).
    Get(GetArgs),
    /// Upload a JSON pack to the gateway.
    Install(InstallArgs),
    /// Remove an installed pack (English is server-protected).
    Uninstall(UninstallArgs),
    /// LLM-backed translation — generate a pack on the fly.
    Translate(TranslateArgs),
}

#[derive(Debug, Clone, Args)]
pub struct GetArgs {
    /// BCP-47 locale (e.g. `ja`, `fr-CA`, `zh-CN`).
    pub locale: String,
}

#[derive(Debug, Clone, Args)]
pub struct InstallArgs {
    /// Path to a JSON file shaped as `{ locale, label, strings }`.
    pub file: String,
}

#[derive(Debug, Clone, Args)]
pub struct UninstallArgs {
    pub locale: String,
}

#[derive(Debug, Clone, Args)]
pub struct TranslateArgs {
    /// BCP-47 target locale (e.g. `ja`, `pt-BR`, `zh-TW`).
    #[arg(long)]
    pub target: String,

    /// Own-script display label for the new pack (e.g. `日本語`).
    /// Defaults to the locale code.
    #[arg(long)]
    pub label: Option<String>,

    /// Source locale to translate FROM. Defaults to `en`.
    #[arg(long)]
    pub source: Option<String>,

    /// Style hint passed to the translator: `concise`, `formal`,
    /// or `casual`. Defaults to `concise`.
    #[arg(long)]
    pub style: Option<String>,

    /// Free-form product context the LLM uses to pick register.
    /// Quote it: `--context "Hermon AI is a B2B platform for ..."`
    #[arg(long)]
    pub context: Option<String>,

    /// Model id to dispatch through the gateway's LlmRouter. Defaults
    /// to the gateway's configured default (Anthropic if available,
    /// then OpenAI, then Gemini, else local Ollama). Examples:
    /// `claude-sonnet-4-20250514`, `gpt-4o-mini`, `gemini-2.0-flash-exp`,
    /// `provider:hermon-local:llama-3.1-8b-instruct`.
    #[arg(long)]
    pub model: Option<String>,

    /// Upsert the resulting pack into the gateway database immediately.
    /// Without this flag the pack is only returned (and optionally
    /// written to `--out`) so you can review before committing.
    #[arg(long)]
    pub install: bool,

    /// Write the pack JSON to a file alongside the gateway response.
    /// Handy for `git`-tracking translations:
    ///   wish i18n translate --target fr-CA --out i18n/fr-CA.json
    #[arg(long)]
    pub out: Option<String>,
}
