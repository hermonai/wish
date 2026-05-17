//! `wish project` subcommand surface.
//!
//! Talks directly to the Hermon gateway's `/v1/projects` endpoints from
//! migration `0008_projects.sql` — a "project" is one local codebase
//! the user has opened, with the canonical SDLC commands (build / test
//! / run / lint / format) bound to it.
//!
//! Subcommands map 1:1 to REST verbs, plus a `run` convenience that
//! shells out to the resolved per-project command:
//!
//!   wish project list
//!   wish project add <root_path> [--language rust] [--build "cargo build"] ...
//!   wish project show <id|name>
//!   wish project rm <id|name>
//!   wish project run <build|test|run|lint|format> [name]
//!
//! The CLI implementation lives in `app::ai::agent_sdk::project` (mirroring
//! `admin`, `task`, etc). This file defines the clap shape only.

use clap::{Args, Subcommand};

#[derive(Debug, Clone, Subcommand)]
pub enum ProjectCommand {
    /// List every project bookmark on the signed-in account.
    List,
    /// Create or upsert a project bookmark.
    Add(AddArgs),
    /// Print one project by name or id.
    Show(ShowArgs),
    /// Delete a project bookmark.
    Rm(ShowArgs),
    /// Run one of the configured SDLC commands inside the project root.
    Run(RunArgs),
}

#[derive(Debug, Clone, Args)]
pub struct AddArgs {
    /// Absolute path to the project root. Re-running `add` with the
    /// same `--root` upserts the rest of the fields (per the gateway's
    /// ON CONFLICT (user_id, root_path) clause).
    pub root: String,

    /// Human-facing name. Defaults to the basename of `--root`.
    #[arg(long)]
    pub name: Option<String>,

    /// Free-form language tag (e.g. `rust`, `typescript`).
    #[arg(long)]
    pub language: Option<String>,

    /// Shell command for the IDE's Build button.
    #[arg(long)]
    pub build: Option<String>,

    /// Shell command for the IDE's Test button.
    #[arg(long)]
    pub test: Option<String>,

    /// Shell command for the IDE's Run button.
    #[arg(long)]
    pub run: Option<String>,

    /// Shell command for the IDE's Lint button.
    #[arg(long)]
    pub lint: Option<String>,

    /// Shell command for the IDE's Format button.
    #[arg(long)]
    pub format: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct ShowArgs {
    /// Project id (UUID) or `name`. Names are matched case-insensitively
    /// against the list returned by `wish project list`.
    pub project: String,
}

#[derive(Debug, Clone, Args)]
pub struct RunArgs {
    /// One of `build`, `test`, `run`, `lint`, `format`.
    pub command: String,

    /// Project name or id. If omitted, the most-recently-updated
    /// project is used.
    pub project: Option<String>,
}
