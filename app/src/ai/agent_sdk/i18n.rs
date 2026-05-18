//! `wish i18n` runner — talks to the Hermon gateway's `/v1/i18n*` API.
//!
//! Subcommand wiring lives here; the clap shape is in
//! `crates/wish_cli/src/i18n.rs`. The HTTP layer reuses the same bearer
//! credential `wish login --email` stores, so no new auth flow.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use wishui::{platform::TerminationMode, AppContext, SingletonEntity};

use wish_cli::i18n::{GetArgs, I18nCommand, InstallArgs, TranslateArgs, UninstallArgs};

use crate::auth::AuthStateProvider;
use crate::server::hermon_auth;

pub fn run(ctx: &mut AppContext, command: I18nCommand) -> Result<()> {
    let bearer = bearer_from_auth_state(ctx)?;
    let api_url = hermon_auth::api_url();
    let result = match command {
        I18nCommand::Locales => list_locales(&api_url, &bearer),
        I18nCommand::Get(a) => get_pack(&api_url, &bearer, a),
        I18nCommand::Install(a) => install_pack(&api_url, &bearer, a),
        I18nCommand::Uninstall(a) => uninstall_pack(&api_url, &bearer, a),
        I18nCommand::Translate(a) => translate_pack(&api_url, &bearer, a),
    };
    let term = result.map_err(|e| anyhow!(format!("{e:#}")));
    ctx.terminate_app(
        TerminationMode::ForceTerminate,
        if term.is_err() {
            Some(term.map(|_| ()))
        } else {
            None
        },
    );
    Ok(())
}

fn bearer_from_auth_state(ctx: &AppContext) -> Result<String> {
    let auth_state = AuthStateProvider::as_ref(ctx).get();
    let creds = auth_state
        .credentials()
        .context("not signed in — run `wish login --email <you@example.com>` first")?;
    match creds {
        crate::auth::credentials::Credentials::Bearer(token) => Ok(token),
        crate::auth::credentials::Credentials::ApiKey { key, .. } => Ok(key),
        _ => Err(anyhow!(
            "current credentials aren't Hermon-native; run `wish login --email <you@example.com>` to switch"
        )),
    }
}

/// Long enough that even a 175-string Anthropic call doesn't get cut.
/// The gateway itself caps the LLM at 32k output tokens; client side
/// just needs to be permissive.
fn http() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .context("build http client")
}

#[derive(Deserialize)]
struct LocalesResponse {
    items: Vec<LocaleEntry>,
}

#[derive(Deserialize)]
struct LocaleEntry {
    locale: String,
    label: String,
    installed: bool,
    string_count: i64,
}

fn list_locales(api_url: &str, bearer: &str) -> Result<()> {
    let resp = http()?
        .get(format!("{api_url}/v1/i18n/locales"))
        .bearer_auth(bearer)
        .send()
        .context("GET /v1/i18n/locales")?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(anyhow!("HTTP {status}: {body}"));
    }
    let list: LocalesResponse = resp.json().context("parse /v1/i18n/locales")?;
    println!("{:<10} {:<6} {:<8} {}", "LOCALE", "ROWS", "STATE", "LABEL");
    for e in list.items {
        let state = if e.installed { "live" } else { "—" };
        println!(
            "{:<10} {:<6} {:<8} {}",
            e.locale, e.string_count, state, e.label
        );
    }
    Ok(())
}

#[derive(Deserialize, Serialize)]
struct PackResponse {
    locale: String,
    label: String,
    strings: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fallback_to: Option<String>,
}

fn get_pack(api_url: &str, bearer: &str, args: GetArgs) -> Result<()> {
    let resp = http()?
        .get(format!(
            "{api_url}/v1/i18n?locale={}",
            urlencoding(&args.locale)
        ))
        .bearer_auth(bearer)
        .send()
        .context("GET /v1/i18n")?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(anyhow!("HTTP {status}: {body}"));
    }
    let pack: PackResponse = resp.json().context("parse /v1/i18n")?;
    let out = serde_json::json!({
        "locale": pack.locale,
        "label": pack.label,
        "strings": pack.strings,
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    if let Some(fb) = pack.fallback_to {
        eprintln!(
            "note: locale `{}` not installed; gateway served the `{}` fallback. \
             Generate one with `wish i18n translate --target {} --install`.",
            args.locale, fb, args.locale
        );
    }
    Ok(())
}

#[derive(Serialize)]
struct InstallBody {
    locale: String,
    label: String,
    strings: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct OkResponse {
    ok: bool,
    locale: String,
    string_count: usize,
}

fn install_pack(api_url: &str, bearer: &str, args: InstallArgs) -> Result<()> {
    let bytes = std::fs::read(&args.file)
        .with_context(|| format!("read {}", args.file))?;
    #[derive(Deserialize)]
    struct InFile {
        locale: String,
        label: String,
        strings: BTreeMap<String, String>,
    }
    let parsed: InFile = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse {}", args.file))?;
    let body = InstallBody {
        locale: parsed.locale.clone(),
        label: parsed.label,
        strings: parsed.strings,
    };
    let resp = http()?
        .post(format!("{api_url}/v1/admin/i18n"))
        .bearer_auth(bearer)
        .json(&body)
        .send()
        .context("POST /v1/admin/i18n")?;
    let status = resp.status();
    if !status.is_success() {
        let txt = resp.text().unwrap_or_default();
        return Err(anyhow!("HTTP {status}: {txt}"));
    }
    let ok: OkResponse = resp.json()?;
    println!(
        "Installed `{}` ({} strings){}",
        ok.locale,
        ok.string_count,
        if ok.ok { "" } else { " — server returned ok: false" }
    );
    Ok(())
}

fn uninstall_pack(api_url: &str, bearer: &str, args: UninstallArgs) -> Result<()> {
    let resp = http()?
        .delete(format!(
            "{api_url}/v1/admin/i18n/{}",
            urlencoding(&args.locale)
        ))
        .bearer_auth(bearer)
        .send()
        .context("DELETE /v1/admin/i18n/:locale")?;
    let status = resp.status();
    if !status.is_success() {
        let txt = resp.text().unwrap_or_default();
        return Err(anyhow!("HTTP {status}: {txt}"));
    }
    println!("Uninstalled `{}`.", args.locale);
    Ok(())
}

#[derive(Serialize)]
struct TranslateBody<'a> {
    target_locale: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_locale: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    style: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    install: bool,
}

#[derive(Deserialize, Serialize)]
struct TranslateResponse {
    target_locale: String,
    label: String,
    strings: BTreeMap<String, String>,
    missing: Vec<String>,
    placeholder_drift: Vec<String>,
    complete: bool,
    installed: bool,
    model_used: String,
}

fn translate_pack(api_url: &str, bearer: &str, args: TranslateArgs) -> Result<()> {
    let body = TranslateBody {
        target_locale: &args.target,
        label: args.label.as_deref(),
        source_locale: args.source.as_deref(),
        style: args.style.as_deref(),
        context: args.context.as_deref(),
        model: args.model.as_deref(),
        install: args.install,
    };
    eprintln!(
        "Asking the gateway to translate to `{}` (this can take 30–90s)…",
        args.target
    );
    let resp = http()?
        .post(format!("{api_url}/v1/admin/i18n/translate"))
        .bearer_auth(bearer)
        .json(&body)
        .send()
        .context("POST /v1/admin/i18n/translate")?;
    let status = resp.status();
    if !status.is_success() {
        let txt = resp.text().unwrap_or_default();
        return Err(anyhow!("HTTP {status}: {txt}"));
    }
    let result: TranslateResponse = resp.json().context("parse translate response")?;
    let pack = serde_json::json!({
        "locale": result.target_locale,
        "label": result.label,
        "strings": result.strings,
    });
    if let Some(out_path) = args.out.as_deref() {
        std::fs::write(out_path, serde_json::to_vec_pretty(&pack)?)
            .with_context(|| format!("write {out_path}"))?;
        eprintln!("Wrote {out_path}.");
    } else {
        println!("{}", serde_json::to_string_pretty(&pack)?);
    }
    eprintln!(
        "{verb} `{loc}` via {model}: {count} strings, complete={complete}{drift_note}{install_note}",
        verb = if result.installed { "Installed" } else { "Generated" },
        loc = result.target_locale,
        model = result.model_used,
        count = result.strings.len(),
        complete = result.complete,
        drift_note = if !result.missing.is_empty() || !result.placeholder_drift.is_empty() {
            format!(
                " (missing: {}, drift: {})",
                result.missing.len(),
                result.placeholder_drift.len()
            )
        } else {
            String::new()
        },
        install_note = if !result.installed && args.install {
            " — server reported installed=false; inspect the response above"
        } else {
            ""
        },
    );
    if !result.missing.is_empty() {
        eprintln!("missing keys:");
        for k in &result.missing {
            eprintln!("  {k}");
        }
    }
    if !result.placeholder_drift.is_empty() {
        eprintln!("placeholder drift (excluded from install):");
        for k in &result.placeholder_drift {
            eprintln!("  {k}");
        }
    }
    Ok(())
}

/// Tiny URL-encoder for the few cases we need (no spaces, only BCP-47
/// chars and the occasional hyphen). Anything fancier would pull in
/// `urlencoding` as a runtime dep for one call site.
fn urlencoding(s: &str) -> String {
    s.replace('/', "%2F").replace(' ', "%20")
}
