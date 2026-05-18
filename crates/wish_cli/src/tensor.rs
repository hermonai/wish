//! `wish tensor` subcommand surface.
//!
//! The CLI half of the URE × wishUI tensor data plane. Talks to the
//! Hermon gateway's `/v1/tensors` endpoints from migration
//! `0009_tensors.sql`. A "tensor" here is one row in that table — a
//! shape + dtype + raw bytes blob, optionally addressed by a stable
//! per-user `name`.
//!
//! Subcommands map closely to REST verbs:
//!
//!   wish tensor list
//!   wish tensor push <file> --dims H,W,... --dtype f32 [--name <name>] [--label <text>]
//!   wish tensor show <id|name>
//!   wish tensor pull <id|name> [--out <path>]
//!   wish tensor rm   <id|name>
//!   wish tensor render <id|name> [--perspective <p>]
//!
//! Why pull/push separate from render
//! ──────────────────────────────────
//! `push` and `pull` are scriptable plumbing — they round-trip raw
//! bytes so users can wire tensors into Python notebooks / shell
//! pipelines. `render` is a convenience that pulls + opens the native
//! viewer, mirroring `wish-world render tensor` but reading from
//! Hermon instead of building synthetic examples.
//!
//! The clap shape lives here; the actual reqwest + filesystem code
//! lives in `app::ai::agent_sdk::tensor` so this crate stays slim
//! enough to embed into other consumers.

use clap::{Args, Subcommand};

#[derive(Debug, Clone, Subcommand)]
pub enum TensorCommand {
    /// List every tensor on the signed-in account, most-recent first.
    /// Output includes id, name, shape, dtype, byte_size and sha256.
    List,

    /// Upload a tensor's bytes from a local file.
    ///
    /// The file is read raw — row-major, little-endian, no header —
    /// because that's the layout the gateway and `wish-canvas-core`
    /// agree on. For .npy / .safetensors files, extract a single
    /// tensor first; multi-tensor formats are deliberately not
    /// flattened by this command (that's the upcoming `wish tensor
    /// import` job).
    Push(PushArgs),

    /// Print one tensor's metadata by id or name.
    Show(ShowArgs),

    /// Download a tensor's bytes by id or name.
    Pull(PullArgs),

    /// Delete a tensor (metadata and bytes both — `ON DELETE CASCADE`).
    Rm(ShowArgs),

    /// Pull a tensor from Hermon and open it in the native viewer as
    /// an inline heatmap on a canvas. Convenience wrapper over `pull`
    /// + `wish-world render tensor`.
    Render(RenderArgs),
}

#[derive(Debug, Clone, Args)]
pub struct PushArgs {
    /// Path to a file containing row-major little-endian bytes.
    /// `length_in_bytes` must equal `dims.product() * dtype.byte_size()`
    /// — the gateway rejects mismatches with a 400.
    pub file: String,

    /// Comma-separated shape. Examples: `768`, `24,24,4`, `768,768`.
    /// Empty is fine for a scalar (`--dims ""`).
    #[arg(long)]
    pub dims: String,

    /// Element dtype. One of `f32`, `f64`, `i32`, `i64`, `u8`, `bool`
    /// — matches `wish_canvas_core::TensorDType` exactly.
    #[arg(long)]
    pub dtype: String,

    /// Optional stable name. `wish tensor pull <name>` resolves to
    /// this row; the gateway enforces `(user_id, name)` uniqueness so
    /// a second push with the same name upserts.
    #[arg(long)]
    pub name: Option<String>,

    /// Human label rendered next to the tensor in the wishUI / CLI
    /// listings. Free text.
    #[arg(long)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct ShowArgs {
    /// Tensor id (UUID) or `name`. Names are matched case-sensitively
    /// against the gateway's `(user_id, name)` index.
    pub tensor: String,
}

#[derive(Debug, Clone, Args)]
pub struct PullArgs {
    /// Tensor id or name. See `ShowArgs::tensor`.
    pub tensor: String,

    /// Output file path. Defaults to `<id>.bin` in the current
    /// directory. Existing files are overwritten.
    #[arg(long)]
    pub out: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct RenderArgs {
    /// Tensor id or name.
    pub tensor: String,

    /// Perspective slug to open the viewer with. Defaults to
    /// `engineering`. See `wish-world render demo --perspective …` for
    /// the full list.
    #[arg(long)]
    pub perspective: Option<String>,
}
