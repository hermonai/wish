use std::path::Path;
use std::sync::Arc;

use crate::language_server_candidate::{LanguageServerCandidate, LanguageServerMetadata};
use crate::CommandBuilder;
use async_trait::async_trait;

/// A generic LSP server candidate that works for servers with simple
/// binary-on-PATH installation patterns.
///
/// Most newly-added language servers follow the same pattern:
/// 1. Check for project marker files to suggest for a repo
/// 2. Check if the binary is on PATH via `--version`
/// 3. No custom data_dir installation (user installs via their
///    language's package manager)
///
/// This struct captures that common pattern so we don't need a separate
/// file for every single LSP server.
pub struct GenericLspCandidate {
    #[allow(dead_code)]
    client: Arc<http_client::Client>,
    /// The binary name to check on PATH (e.g., "solargraph", "zls").
    binary_name: &'static str,
    /// Args to pass for a version/health check (e.g., &["--version"]).
    version_args: &'static [&'static str],
    /// Project marker files whose presence indicates this language is used
    /// (e.g., &["pom.xml", "build.gradle"] for Java).
    project_markers: &'static [&'static str],
    /// File extensions to look for in the repo root when no marker file
    /// is found (e.g., &["java"] for Java).
    source_extensions: &'static [&'static str],
}

impl GenericLspCandidate {
    pub fn new(
        client: Arc<http_client::Client>,
        binary_name: &'static str,
        version_args: &'static [&'static str],
        project_markers: &'static [&'static str],
        source_extensions: &'static [&'static str],
    ) -> Self {
        Self {
            client,
            binary_name,
            version_args,
            project_markers,
            source_extensions,
        }
    }
}

#[async_trait]
#[cfg(feature = "local_fs")]
impl LanguageServerCandidate for GenericLspCandidate {
    async fn should_suggest_for_repo(&self, path: &Path, _executor: &CommandBuilder) -> bool {
        // Check for project marker files
        if self
            .project_markers
            .iter()
            .any(|marker| path.join(marker).exists())
        {
            return true;
        }

        // Check for source files with matching extensions in the repo root
        if !self.source_extensions.is_empty() {
            if let Ok(entries) = std::fs::read_dir(path) {
                return entries.flatten().any(|entry| {
                    let file_path = entry.path();
                    file_path.is_file()
                        && file_path
                            .extension()
                            .and_then(|ext| ext.to_str())
                            .is_some_and(|ext| self.source_extensions.contains(&ext))
                });
            }
        }

        false
    }

    async fn is_installed_in_data_dir(&self, _executor: &CommandBuilder) -> bool {
        // Generic servers don't support custom data_dir installation
        false
    }

    async fn is_installed_on_path(&self, executor: &CommandBuilder) -> bool {
        let mut cmd = executor.command(self.binary_name);
        for arg in self.version_args {
            cmd.arg(*arg);
        }
        cmd.output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    async fn install(
        &self,
        _metadata: LanguageServerMetadata,
        _executor: &CommandBuilder,
    ) -> anyhow::Result<()> {
        anyhow::bail!(
            "Automatic installation is not supported for {}. \
             Please install it manually using your language's package manager.",
            self.binary_name
        )
    }

    async fn fetch_latest_server_metadata(&self) -> anyhow::Result<LanguageServerMetadata> {
        anyhow::bail!(
            "Automatic metadata fetching is not supported for {}. \
             Install or update manually.",
            self.binary_name
        )
    }
}

#[async_trait]
#[cfg(not(feature = "local_fs"))]
impl LanguageServerCandidate for GenericLspCandidate {
    async fn should_suggest_for_repo(&self, _path: &Path, _executor: &CommandBuilder) -> bool {
        false
    }

    async fn is_installed_in_data_dir(&self, _executor: &CommandBuilder) -> bool {
        false
    }

    async fn is_installed_on_path(&self, _executor: &CommandBuilder) -> bool {
        false
    }

    async fn install(
        &self,
        _metadata: LanguageServerMetadata,
        _executor: &CommandBuilder,
    ) -> anyhow::Result<()> {
        anyhow::bail!("LSP installation is not supported on this platform")
    }

    async fn fetch_latest_server_metadata(&self) -> anyhow::Result<LanguageServerMetadata> {
        anyhow::bail!("LSP metadata fetching is not supported on this platform")
    }
}
