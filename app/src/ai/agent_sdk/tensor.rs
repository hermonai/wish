//! `wish tensor` CLI runner — talks to the Hermon gateway's
//! `/v1/tensors` surface (migration 0009).
//!
//! Mirrors the layout of `agent_sdk::project`: the clap shape lives
//! in `wish_cli::tensor`; this file does the reqwest + filesystem
//! work and is registered as a sibling module in `mod.rs`.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use wishui::{platform::TerminationMode, AppContext, SingletonEntity};

use wish_cli::tensor::{PullArgs, PushArgs, RenderArgs, ShowArgs, TensorCommand};

use crate::auth::AuthStateProvider;
use crate::server::hermon_auth;

/// Server wire shape — matches `routes::tensors::Tensor` in hermon-gateway.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct Tensor {
    id: String,
    user_id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    label: Option<String>,
    dims: serde_json::Value,
    dtype: String,
    element_count: i64,
    byte_size: i64,
    #[serde(default)]
    extras: serde_json::Value,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TensorList {
    items: Vec<Tensor>,
}

pub fn run(ctx: &mut AppContext, command: TensorCommand) -> Result<()> {
    let bearer = bearer_from_auth_state(ctx)?;
    let api_url = hermon_auth::api_url();
    let result = match command {
        TensorCommand::List => list_tensors(&api_url, &bearer),
        TensorCommand::Push(args) => push_tensor(&api_url, &bearer, args),
        TensorCommand::Show(args) => show_tensor(&api_url, &bearer, args),
        TensorCommand::Pull(args) => pull_tensor(&api_url, &bearer, args),
        TensorCommand::Rm(args) => rm_tensor(&api_url, &bearer, args),
        TensorCommand::Render(args) => render_tensor(&api_url, &bearer, args),
    };
    let term_result = result.map_err(|e| anyhow!(format!("{e:#}")));
    ctx.terminate_app(
        TerminationMode::ForceTerminate,
        if term_result.is_err() {
            Some(term_result.map(|_| ()))
        } else {
            None
        },
    );
    Ok(())
}

/// Same shape as `project::bearer_from_auth_state` — see the comment
/// there. Either form (Bearer or ApiKey) the gateway's `/v1/tensors`
/// handlers accept.
fn bearer_from_auth_state(ctx: &AppContext) -> Result<String> {
    let auth_state = AuthStateProvider::as_ref(ctx).get();
    let creds = auth_state.credentials().context(
        "not signed in — run `wish login --email <you@example.com>` first",
    )?;
    match creds {
        crate::auth::credentials::Credentials::Bearer(token) => Ok(token),
        crate::auth::credentials::Credentials::ApiKey { key, .. } => Ok(key),
        _ => Err(anyhow!(
            "current credentials aren't Hermon-native; run `wish login --email <you@example.com>` to switch"
        )),
    }
}

fn http() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        // Tensor data can be large — 60s is enough for a few hundred
        // MiB on a typical home connection. Anything bigger should
        // use the upcoming streaming variant.
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .context("build http client")
}

// ── list / show / rm ────────────────────────────────────────────────

fn list_tensors(api_url: &str, bearer: &str) -> Result<()> {
    let resp = http()?
        .get(format!("{api_url}/v1/tensors"))
        .bearer_auth(bearer)
        .send()
        .context("GET /v1/tensors")?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(anyhow!("HTTP {status}: {body}"));
    }
    let list: TensorList = resp.json().context("parse /v1/tensors")?;
    if list.items.is_empty() {
        println!("No tensors yet. Upload one with `wish tensor push <file> --dims H,W --dtype f32`.");
        return Ok(());
    }
    for t in &list.items {
        let id_or_name = t.name.clone().unwrap_or_else(|| t.id.clone());
        println!(
            "{id_or_name}  {}  {}  ({} bytes)",
            shape_string(&t.dims),
            t.dtype,
            t.byte_size
        );
        if let Some(label) = &t.label {
            println!("    label: {label}");
        }
        if let Some(sha) = &t.sha256 {
            println!("    sha256: {sha}");
        }
        println!("    id: {}", t.id);
    }
    Ok(())
}

fn show_tensor(api_url: &str, bearer: &str, args: ShowArgs) -> Result<()> {
    let t = resolve_tensor(api_url, bearer, &args.tensor)?;
    println!(
        "{}\n  shape: {}\n  dtype: {}\n  elements: {}\n  bytes: {}",
        t.name.clone().unwrap_or_else(|| t.id.clone()),
        shape_string(&t.dims),
        t.dtype,
        t.element_count,
        t.byte_size,
    );
    if let Some(label) = &t.label {
        println!("  label: {label}");
    }
    if let Some(sha) = &t.sha256 {
        println!("  sha256: {sha}");
    }
    println!("  id: {}\n  created_at: {}\n  updated_at: {}", t.id, t.created_at, t.updated_at);
    Ok(())
}

fn rm_tensor(api_url: &str, bearer: &str, args: ShowArgs) -> Result<()> {
    let t = resolve_tensor(api_url, bearer, &args.tensor)?;
    let resp = http()?
        .delete(format!("{api_url}/v1/tensors/{}", t.id))
        .bearer_auth(bearer)
        .send()
        .context("DELETE /v1/tensors/:id")?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(anyhow!("HTTP {status}: {body}"));
    }
    println!("deleted {}", t.id);
    Ok(())
}

// ── push ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct CreateBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<&'a str>,
    dims: Vec<u64>,
    dtype: &'a str,
    data_b64: String,
}

fn push_tensor(api_url: &str, bearer: &str, args: PushArgs) -> Result<()> {
    let dims = parse_dims(&args.dims)?;
    let dtype = args.dtype.as_str();
    let elem_size = dtype_byte_size(dtype).ok_or_else(|| {
        anyhow!(
            "unsupported dtype: {dtype} (expected one of f32, f64, i32, i64, u8, bool)"
        )
    })?;
    let elem_count: u64 = dims.iter().copied().try_fold(1u64, u64::checked_mul).ok_or_else(
        || anyhow!("dims product overflows u64"),
    )?;
    let expected_bytes = (elem_count as usize)
        .checked_mul(elem_size)
        .ok_or_else(|| anyhow!("byte_size overflows usize"))?;
    let bytes = std::fs::read(&args.file)
        .with_context(|| format!("read {}", args.file))?;
    if bytes.len() != expected_bytes {
        return Err(anyhow!(
            "file is {} bytes but shape × dtype = {} bytes — did you pass the right --dims / --dtype?",
            bytes.len(),
            expected_bytes
        ));
    }
    let data_b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let body = CreateBody {
        name: args.name.as_deref(),
        label: args.label.as_deref(),
        dims,
        dtype,
        data_b64,
    };
    let resp = http()?
        .post(format!("{api_url}/v1/tensors"))
        .bearer_auth(bearer)
        .json(&body)
        .send()
        .context("POST /v1/tensors")?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(anyhow!("HTTP {status}: {body}"));
    }
    let t: Tensor = resp.json().context("parse /v1/tensors create response")?;
    println!(
        "uploaded {} ({} bytes, sha256 {})",
        t.name.clone().unwrap_or_else(|| t.id.clone()),
        t.byte_size,
        t.sha256.clone().unwrap_or_default(),
    );
    println!("id: {}", t.id);
    Ok(())
}

// ── pull ─────────────────────────────────────────────────────────────

fn pull_tensor(api_url: &str, bearer: &str, args: PullArgs) -> Result<()> {
    let t = resolve_tensor(api_url, bearer, &args.tensor)?;
    let resp = http()?
        .get(format!("{api_url}/v1/tensors/{}/data", t.id))
        .bearer_auth(bearer)
        .send()
        .context("GET /v1/tensors/:id/data")?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(anyhow!("HTTP {status}: {body}"));
    }
    // Pull the integrity header before consuming the body so we can
    // verify after read without depending on a streaming hash.
    let server_sha = resp
        .headers()
        .get("x-tensor-sha256")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());
    let bytes = resp.bytes().context("read tensor body")?;
    if let Some(server) = &server_sha {
        let local = hex_sha256(&bytes);
        if &local != server {
            return Err(anyhow!(
                "downloaded bytes hash mismatch: server says {server}, computed {local}"
            ));
        }
    }
    let out_path = args
        .out
        .unwrap_or_else(|| format!("{}.bin", t.name.clone().unwrap_or_else(|| t.id.clone())));
    std::fs::write(&out_path, &bytes).with_context(|| format!("write {out_path}"))?;
    println!(
        "downloaded {} bytes to {out_path}{}",
        bytes.len(),
        if server_sha.is_some() { " (sha256 verified)" } else { "" },
    );
    Ok(())
}

// ── render ───────────────────────────────────────────────────────────

fn render_tensor(api_url: &str, bearer: &str, args: RenderArgs) -> Result<()> {
    // Pull to a temp file under the system temp dir, then exec
    // `wish-world render tensor-file` — except that subcommand doesn't
    // exist yet (Session 4's `render tensor` builds synthetic data).
    // For v1 we just pull and tell the user the path; once the
    // `tensor-file <path> --dims A,B --dtype f32` flavor lands we can
    // chain it here.
    let t = resolve_tensor(api_url, bearer, &args.tensor)?;
    let tmp = std::env::temp_dir().join(format!("wish-tensor-{}.bin", t.id));
    let resp = http()?
        .get(format!("{api_url}/v1/tensors/{}/data", t.id))
        .bearer_auth(bearer)
        .send()
        .context("GET /v1/tensors/:id/data")?;
    let bytes = resp.bytes().context("read tensor body")?;
    std::fs::write(&tmp, &bytes).with_context(|| format!("write {}", tmp.display()))?;
    eprintln!(
        "tensor pulled to {} ({} bytes)\n\
         To open it as a heatmap once `wish-world render tensor-file` lands:\n  \
         wish-world render tensor-file {} --dims {} --dtype {}{}",
        tmp.display(),
        bytes.len(),
        tmp.display(),
        shape_string(&t.dims),
        t.dtype,
        match &args.perspective {
            Some(p) => format!(" --perspective {p}"),
            None => String::new(),
        },
    );
    Ok(())
}

// ── helpers ──────────────────────────────────────────────────────────

/// Look up a tensor by either its UUID or stable `name`. The gateway
/// only resolves by id; we fall back to fetching `/v1/tensors` and
/// filtering client-side when the input isn't a valid UUID. That's a
/// linear scan but tensor lists are short (a user with thousands of
/// tensors would be addressing them by id anyway).
fn resolve_tensor(api_url: &str, bearer: &str, ident: &str) -> Result<Tensor> {
    if looks_like_uuid(ident) {
        let resp = http()?
            .get(format!("{api_url}/v1/tensors/{ident}"))
            .bearer_auth(bearer)
            .send()
            .context("GET /v1/tensors/:id")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(anyhow!("HTTP {status}: {body}"));
        }
        return resp.json().context("parse /v1/tensors/:id");
    }
    let resp = http()?
        .get(format!("{api_url}/v1/tensors"))
        .bearer_auth(bearer)
        .send()
        .context("GET /v1/tensors")?;
    let list: TensorList = resp.json().context("parse /v1/tensors")?;
    list.items
        .into_iter()
        .find(|t| t.name.as_deref() == Some(ident))
        .ok_or_else(|| anyhow!("no tensor named {ident:?} found"))
}

fn looks_like_uuid(s: &str) -> bool {
    // Cheap pre-check: 8-4-4-4-12 hex with dashes. Avoids round-tripping
    // through uuid::Uuid which isn't a direct dep of this module.
    let bytes = s.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (i, &b) in bytes.iter().enumerate() {
        let is_dash = matches!(i, 8 | 13 | 18 | 23);
        if is_dash {
            if b != b'-' {
                return false;
            }
        } else if !b.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn shape_string(dims: &serde_json::Value) -> String {
    match dims.as_array() {
        Some(a) if a.is_empty() => "[scalar]".into(),
        Some(a) => format!(
            "[{}]",
            a.iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ),
        None => dims.to_string(),
    }
}

fn parse_dims(s: &str) -> Result<Vec<u64>> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(Vec::new());
    }
    s.split(',')
        .map(|part| {
            part.trim().parse::<u64>().with_context(|| {
                format!("dims: failed to parse {part:?} as a non-negative integer")
            })
        })
        .collect()
}

fn dtype_byte_size(dtype: &str) -> Option<usize> {
    match dtype {
        "f32" | "i32" => Some(4),
        "f64" | "i64" => Some(8),
        "u8" | "bool" => Some(1),
        _ => None,
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    // Direct dep on sha2 isn't available in this crate; the wish app
    // pulls hmac-sha256 through reqwest's rustls-tls feature though,
    // and `sha2::Sha256` is reachable via a transitive re-export.
    // Fall back to a tiny hand-rolled hash if not — but sha2 IS in
    // the workspace deps (gateway uses it), so adding it explicitly
    // would be the right move if reuse becomes a theme.
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dims_handles_empty_and_csv() {
        assert_eq!(parse_dims("").unwrap(), Vec::<u64>::new());
        assert_eq!(parse_dims("   ").unwrap(), Vec::<u64>::new());
        assert_eq!(parse_dims("768").unwrap(), vec![768]);
        assert_eq!(parse_dims("24,24,4").unwrap(), vec![24, 24, 4]);
        assert_eq!(parse_dims("768, 768").unwrap(), vec![768, 768]);
    }

    #[test]
    fn parse_dims_rejects_garbage() {
        assert!(parse_dims("768,abc").is_err());
        assert!(parse_dims("-1").is_err());
    }

    #[test]
    fn dtype_byte_size_matches_canvas_core() {
        // Same table as wish_canvas_core::TensorDType::byte_size.
        assert_eq!(dtype_byte_size("f32"), Some(4));
        assert_eq!(dtype_byte_size("f64"), Some(8));
        assert_eq!(dtype_byte_size("i32"), Some(4));
        assert_eq!(dtype_byte_size("i64"), Some(8));
        assert_eq!(dtype_byte_size("u8"), Some(1));
        assert_eq!(dtype_byte_size("bool"), Some(1));
        assert_eq!(dtype_byte_size("bfloat16"), None);
    }

    #[test]
    fn looks_like_uuid_matches_real_uuids() {
        assert!(looks_like_uuid("018e2f51-d23e-7e84-9c5a-2f0fbd6c12ee"));
        // Wrong length:
        assert!(!looks_like_uuid("018e2f51-d23e-7e84-9c5a-2f0fbd6c"));
        // Dash in the wrong place:
        assert!(!looks_like_uuid("018e2f51d-23e-7e84-9c5a-2f0fbd6c12ee"));
        // Plain names:
        assert!(!looks_like_uuid("wte"));
        assert!(!looks_like_uuid(""));
    }

    #[test]
    fn shape_string_renders_arrays_and_scalars() {
        assert_eq!(shape_string(&serde_json::json!([])), "[scalar]");
        assert_eq!(shape_string(&serde_json::json!([24, 24, 4])), "[24,24,4]");
    }

    #[test]
    fn sha256_known_digest() {
        assert_eq!(
            hex_sha256(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    /// `Path` import sanity — we don't currently use it as a type, but
    /// the standard wish-app pattern includes it. Keep an unused-import
    /// stub silent without disabling lints across the file.
    #[allow(dead_code)]
    fn _path_ref(p: &Path) -> &Path {
        p
    }
}
