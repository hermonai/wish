use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
#[cfg(not(target_arch = "wasm32"))]
use command::r#async::Command;
use lsp_types::{
    ClientCapabilities, ClientInfo, DidChangeWatchedFilesClientCapabilities, GotoCapability,
    HoverClientCapabilities, InitializeParams, MarkupKind, PublishDiagnosticsClientCapabilities,
    TextDocumentClientCapabilities, TextDocumentSyncClientCapabilities, Uri,
    WindowClientCapabilities, WorkDoneProgressParams, WorkspaceClientCapabilities, WorkspaceFolder,
};

use crate::supported_servers::LSPServerType;

/// Result of resolving an LSP server command, including the command and init params.
#[cfg(not(target_arch = "wasm32"))]
pub struct ResolvedLspCommand {
    pub command: Command,
    pub params: InitializeParams,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageId {
    // ── Systems / compiled ──────────────────────────────────────
    Rust,
    C,
    Cpp,
    Go,
    Zig,
    Nim,
    Swift,
    Dart,

    // ── JVM family ──────────────────────────────────────────────
    Java,
    Kotlin,
    Scala,

    // ── .NET family ─────────────────────────────────────────────
    CSharp,
    FSharp,

    // ── Scripting / dynamic ─────────────────────────────────────
    Python,
    Ruby,
    PHP,
    Perl,
    Lua,
    Bash,

    // ── JS / TS ecosystem ───────────────────────────────────────
    TypeScript,
    TypeScriptReact,
    JavaScript,
    JavaScriptReact,
    Svelte,
    Vue,

    // ── Functional / BEAM ───────────────────────────────────────
    Elixir,
    Erlang,
    Haskell,
    OCaml,

    // ── Web markup / style ──────────────────────────────────────
    Html,
    Css,
    Scss,
    Less,

    // ── Data / config ───────────────────────────────────────────
    Json,
    Jsonc,
    Yaml,
    Toml,
    Sql,
    GraphQL,
    Xml,

    // ── Documentation / academic ────────────────────────────────
    Markdown,
    LaTeX,

    // ── Blockchain ──────────────────────────────────────────────
    Solidity,

    // ── DevOps / infra ──────────────────────────────────────────
    Terraform,
    Dockerfile,
    CMake,
}

impl LanguageId {
    pub fn from_path(path: &Path) -> Option<Self> {
        // Try extension-based matching first
        if let Some(extn) = path.extension().and_then(|e| e.to_str()) {
            let result = match extn {
                // ── Systems / compiled ──────────────────────────────
                "rs" => Some(Self::Rust),
                "c" | "C" => Some(Self::C),
                "cc" | "cpp" | "cxx" => Some(Self::Cpp),
                // NOTE: `.h` files are ambiguous (could be C or C++). We
                // map them to Cpp because clangd defaults to C++ for `.h`
                // files anyway. When a compile_commands.json is present,
                // clangd will use the correct language regardless of the
                // languageId we send.
                "h" | "H" | "hh" | "hpp" | "hxx" => Some(Self::Cpp),
                "go" => Some(Self::Go),
                "zig" => Some(Self::Zig),
                "nim" | "nims" | "nimble" => Some(Self::Nim),
                "swift" => Some(Self::Swift),
                "dart" => Some(Self::Dart),

                // ── JVM family ────────────────────────────────────────
                "java" => Some(Self::Java),
                "kt" | "kts" => Some(Self::Kotlin),
                "scala" | "sc" => Some(Self::Scala),

                // ── .NET family ───────────────────────────────────────
                "cs" => Some(Self::CSharp),
                "fs" | "fsx" | "fsi" => Some(Self::FSharp),

                // ── Scripting / dynamic ───────────────────────────────
                "py" | "pyi" => Some(Self::Python),
                "rb" | "erb" | "rake" => Some(Self::Ruby),
                "php" => Some(Self::PHP),
                "pl" | "pm" => Some(Self::Perl),
                "lua" => Some(Self::Lua),
                "sh" | "bash" | "zsh" | "fish" => Some(Self::Bash),

                // ── JS / TS ecosystem ─────────────────────────────────
                "ts" => Some(Self::TypeScript),
                "tsx" => Some(Self::TypeScriptReact),
                "js" | "mjs" | "cjs" => Some(Self::JavaScript),
                "jsx" => Some(Self::JavaScriptReact),
                "svelte" => Some(Self::Svelte),
                "vue" => Some(Self::Vue),

                // ── Functional / BEAM ─────────────────────────────────
                "ex" | "exs" => Some(Self::Elixir),
                "erl" | "hrl" => Some(Self::Erlang),
                "hs" | "lhs" => Some(Self::Haskell),
                "ml" | "mli" => Some(Self::OCaml),

                // ── Web markup / style ────────────────────────────────
                "html" | "htm" => Some(Self::Html),
                "css" => Some(Self::Css),
                "scss" => Some(Self::Scss),
                "less" => Some(Self::Less),

                // ── Data / config ─────────────────────────────────────
                "json" => Some(Self::Json),
                "jsonc" => Some(Self::Jsonc),
                "yaml" | "yml" => Some(Self::Yaml),
                "toml" => Some(Self::Toml),
                "sql" => Some(Self::Sql),
                "graphql" | "gql" => Some(Self::GraphQL),
                "xml" | "xsl" | "xsd" | "xslt" | "svg" => Some(Self::Xml),

                // ── Documentation / academic ──────────────────────────
                "md" | "markdown" => Some(Self::Markdown),
                "tex" | "bib" | "sty" | "cls" => Some(Self::LaTeX),

                // ── Blockchain ────────────────────────────────────────
                "sol" => Some(Self::Solidity),

                // ── DevOps / infra ────────────────────────────────────
                "tf" | "tfvars" => Some(Self::Terraform),
                "cmake" => Some(Self::CMake),

                _ => None,
            };
            if result.is_some() {
                return result;
            }
        }

        // Filename-based matching (for files without standard extensions)
        let filename = path.file_name()?.to_str()?;
        match filename {
            "Dockerfile" | "dockerfile" | "Containerfile" | "containerfile" => {
                Some(Self::Dockerfile)
            }
            "CMakeLists.txt" => Some(Self::CMake),
            _ => None,
        }
    }

    /// Returns the language identifier as used by LSP.
    /// See: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocumentItem
    pub(crate) fn lsp_language_identifier(&self) -> &'static str {
        match self {
            // Systems / compiled
            LanguageId::Rust => "rust",
            LanguageId::C => "c",
            LanguageId::Cpp => "cpp",
            LanguageId::Go => "go",
            LanguageId::Zig => "zig",
            LanguageId::Nim => "nim",
            LanguageId::Swift => "swift",
            LanguageId::Dart => "dart",
            // JVM
            LanguageId::Java => "java",
            LanguageId::Kotlin => "kotlin",
            LanguageId::Scala => "scala",
            // .NET
            LanguageId::CSharp => "csharp",
            LanguageId::FSharp => "fsharp",
            // Scripting
            LanguageId::Python => "python",
            LanguageId::Ruby => "ruby",
            LanguageId::PHP => "php",
            LanguageId::Perl => "perl",
            LanguageId::Lua => "lua",
            LanguageId::Bash => "shellscript",
            // JS / TS
            LanguageId::TypeScript => "typescript",
            LanguageId::TypeScriptReact => "typescriptreact",
            LanguageId::JavaScript => "javascript",
            LanguageId::JavaScriptReact => "javascriptreact",
            LanguageId::Svelte => "svelte",
            LanguageId::Vue => "vue",
            // Functional / BEAM
            LanguageId::Elixir => "elixir",
            LanguageId::Erlang => "erlang",
            LanguageId::Haskell => "haskell",
            LanguageId::OCaml => "ocaml",
            // Web
            LanguageId::Html => "html",
            LanguageId::Css => "css",
            LanguageId::Scss => "scss",
            LanguageId::Less => "less",
            // Data / config
            LanguageId::Json => "json",
            LanguageId::Jsonc => "jsonc",
            LanguageId::Yaml => "yaml",
            LanguageId::Toml => "toml",
            LanguageId::Sql => "sql",
            LanguageId::GraphQL => "graphql",
            LanguageId::Xml => "xml",
            // Documentation
            LanguageId::Markdown => "markdown",
            LanguageId::LaTeX => "latex",
            // Blockchain
            LanguageId::Solidity => "solidity",
            // DevOps / infra
            LanguageId::Terraform => "terraform",
            LanguageId::Dockerfile => "dockerfile",
            LanguageId::CMake => "cmake",
        }
    }

    /// For now we assume a 1:1 language -> LSP server type. This might change
    /// in the future as we support more configurabilities.
    pub fn server_type(&self) -> LSPServerType {
        match self {
            // ── Original five ───────────────────────────────────────
            LanguageId::Rust => LSPServerType::RustAnalyzer,
            LanguageId::Go => LSPServerType::GoPls,
            LanguageId::Python => LSPServerType::Pyright,
            LanguageId::TypeScript
            | LanguageId::TypeScriptReact
            | LanguageId::JavaScript
            | LanguageId::JavaScriptReact => LSPServerType::TypeScriptLanguageServer,
            LanguageId::C | LanguageId::Cpp => LSPServerType::Clangd,

            // ── Systems / compiled ──────────────────────────────────
            LanguageId::Zig => LSPServerType::Zls,
            LanguageId::Nim => LSPServerType::NimLangServer,
            LanguageId::Swift => LSPServerType::SourceKitLsp,
            LanguageId::Dart => LSPServerType::DartAnalysisServer,

            // ── JVM family ──────────────────────────────────────────
            LanguageId::Java => LSPServerType::Jdtls,
            LanguageId::Kotlin => LSPServerType::KotlinLanguageServer,
            LanguageId::Scala => LSPServerType::Metals,

            // ── .NET family ─────────────────────────────────────────
            LanguageId::CSharp => LSPServerType::OmniSharp,
            LanguageId::FSharp => LSPServerType::FsAutoComplete,

            // ── Scripting / dynamic ─────────────────────────────────
            LanguageId::Ruby => LSPServerType::Solargraph,
            LanguageId::PHP => LSPServerType::Intelephense,
            LanguageId::Perl => LSPServerType::PerlNavigator,
            LanguageId::Lua => LSPServerType::LuaLanguageServer,
            LanguageId::Bash => LSPServerType::BashLanguageServer,

            // ── JS framework components ─────────────────────────────
            LanguageId::Svelte => LSPServerType::SvelteLanguageServer,
            LanguageId::Vue => LSPServerType::VueLanguageServer,

            // ── Functional / BEAM ───────────────────────────────────
            LanguageId::Elixir => LSPServerType::ElixirLs,
            LanguageId::Erlang => LSPServerType::ErlangLs,
            LanguageId::Haskell => LSPServerType::Hls,
            LanguageId::OCaml => LSPServerType::OcamlLsp,

            // ── Web markup / style ──────────────────────────────────
            LanguageId::Html => LSPServerType::HtmlLanguageServer,
            LanguageId::Css | LanguageId::Scss | LanguageId::Less => {
                LSPServerType::CssLanguageServer
            }

            // ── Data / config ───────────────────────────────────────
            LanguageId::Json | LanguageId::Jsonc => LSPServerType::JsonLanguageServer,
            LanguageId::Yaml => LSPServerType::YamlLanguageServer,
            LanguageId::Toml => LSPServerType::Taplo,
            LanguageId::Sql => LSPServerType::Sqls,
            LanguageId::GraphQL => LSPServerType::GraphQLLs,
            LanguageId::Xml => LSPServerType::Lemminx,

            // ── Documentation / academic ────────────────────────────
            LanguageId::Markdown => LSPServerType::Marksman,
            LanguageId::LaTeX => LSPServerType::Texlab,

            // ── Blockchain ──────────────────────────────────────────
            LanguageId::Solidity => LSPServerType::SolidityLanguageServer,

            // ── DevOps / infra ──────────────────────────────────────
            LanguageId::Terraform => LSPServerType::TerraformLs,
            LanguageId::Dockerfile => LSPServerType::DockerfileLs,
            LanguageId::CMake => LSPServerType::CmakeLanguageServer,
        }
    }
}

/// Configuration for spawning an LSP server process.
#[derive(Clone)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub struct LspServerConfig {
    server_type: LSPServerType,
    initial_workspace: PathBuf,
    /// The local PATH variable set when starting the server. This is needed when the app is started
    /// without a shell based parent process.
    /// TODO(kevin): This might not be sufficient for all cases (e.g. user might remove LSP from PATH).
    path_env_var: Option<String>,
    client_name: String,
    /// Shared HTTP client used for LSP installation checks and downloads.
    client: Arc<http_client::Client>,
    /// Optional path relative to the LSP log namespace for server stderr output.
    log_relative_path: Option<PathBuf>,
}

impl fmt::Debug for LspServerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LspServerConfig")
            .field("server_type", &self.server_type)
            .field("initial_workspace", &self.initial_workspace)
            .field("path_env_var", &self.path_env_var)
            .field("client_name", &self.client_name)
            .field("log_relative_path", &self.log_relative_path)
            .finish()
    }
}

impl LspServerConfig {
    pub fn new(
        server_type: LSPServerType,
        initial_workspace: PathBuf,
        path_env_var: Option<String>,
        client_name: String,
        client: Arc<http_client::Client>,
    ) -> Self {
        Self {
            server_type,
            initial_workspace,
            path_env_var,
            client_name,
            client,
            log_relative_path: None,
        }
    }

    /// Sets the relative log path for this server's stderr output.
    pub fn with_log_relative_path(mut self, log_relative_path: PathBuf) -> Self {
        self.log_relative_path = Some(log_relative_path);
        self
    }
    /// Returns the relative log path if configured.
    pub fn log_relative_path(&self) -> Option<&PathBuf> {
        self.log_relative_path.as_ref()
    }

    /// Returns the initial workspace path.
    pub fn initial_workspace(&self) -> &Path {
        &self.initial_workspace
    }

    pub(crate) fn server_name(&self) -> String {
        self.server_type.binary_name().to_string()
    }

    pub(crate) fn languages(&self) -> Vec<LanguageId> {
        self.server_type.languages()
    }

    /// Creates the command and init params for the LSP server.
    ///
    /// PATH takes precedence over custom installations. If the binary is available
    /// and working on PATH, we use that. Otherwise, we fall back to our custom installation.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) async fn command_and_params(self) -> Result<ResolvedLspCommand> {
        // PATH takes precedence - only use custom installation if not working on PATH
        let executor = crate::CommandBuilder::new(self.path_env_var.clone());
        let is_working_on_path = self
            .server_type
            .is_working_on_path(&executor, self.client.clone())
            .await;
        let custom_binary_config = if is_working_on_path {
            // Binary works on PATH, don't use custom installation
            None
        } else {
            // Not working on PATH, check for custom installation
            self.server_type
                .find_installed_binary_config(executor.path_env_var())
                .await
        };

        // Bail early with a clear error instead of attempting to spawn a
        // binary that doesn't exist (which would fail with a confusing
        // "No such file or directory" OS error).
        if !is_working_on_path && custom_binary_config.is_none() {
            anyhow::bail!(
                "{} is not installed. Binary was not found on PATH and no custom installation exists",
                self.server_type.binary_name()
            );
        }

        let mut command = self
            .server_type
            .create_command(custom_binary_config.clone(), &executor);

        // Set the working directory to the workspace root. This is required for
        // LSP servers like rust-analyzer to properly discover the project structure.
        command.current_dir(&self.initial_workspace);

        log::info!(
            "LSP {} starting with custom_binary_config: {:?}",
            self.server_type.binary_name(),
            custom_binary_config
        );

        let params = default_init_params(&self.initial_workspace, self.client_name)?;

        Ok(ResolvedLspCommand { command, params })
    }

    pub(crate) fn server_type(&self) -> LSPServerType {
        self.server_type
    }
}

pub(crate) fn path_to_lsp_uri(path: &Path) -> Result<Uri> {
    if !path.is_absolute() {
        return Err(anyhow::anyhow!("Path must be absolute: {}", path.display()));
    }

    // url::Url::from_file_path handles percent-encoding internally but is not
    // available on WASM. LSP is not supported on WASM either, so the fallback
    // is a simple string concatenation.
    #[cfg(not(target_arch = "wasm32"))]
    {
        let url = url::Url::from_file_path(path).map_err(|()| {
            anyhow::anyhow!("Failed to convert path to file URI: {}", path.display())
        })?;

        // The url crate doesn't encode brackets, but LSP requires them to be
        // percent-encoded (e.g. Next.js [slug].tsx routes).
        let uri_str = url.as_str().replace('[', "%5B").replace(']', "%5D");

        uri_str.parse::<Uri>().map_err(anyhow::Error::from)
    }

    #[cfg(target_arch = "wasm32")]
    {
        let path_str = path.to_string_lossy();
        let uri_string = format!("file://{path_str}");
        uri_string.parse::<Uri>().map_err(anyhow::Error::from)
    }
}

pub(crate) fn lsp_uri_to_path(uri: &Uri) -> Result<PathBuf> {
    // Validate this is a file URI
    let scheme = uri.scheme().map(|s| s.as_str());
    if scheme != Some("file") {
        return Err(anyhow::anyhow!("Invalid file URI: {}", uri.as_str()));
    }

    // Decode percent-encoded characters (e.g., %40 -> @)
    // This is necessary because LSP servers return URL-encoded paths
    let decoded_path = uri
        .path()
        .as_estr()
        .decode()
        .into_string()
        .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in URI path: {e}"))?;

    let mut path_str: &str = decoded_path.as_ref();

    // Windows URIs are formatted like: file:///C:/path/to/file
    // The path component is `/C:/path/to/file`, strip the leading slash.
    if cfg!(windows) {
        path_str = path_str.strip_prefix('/').unwrap_or(path_str);
        return Ok(PathBuf::from(path_str.replace('/', "\\")));
    }

    Ok(PathBuf::from(path_str))
}

fn path_to_workspace_folder(path: &Path) -> Result<WorkspaceFolder> {
    path_to_lsp_uri(path).map(|url| WorkspaceFolder {
        uri: url,
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
    })
}

fn default_client_capabilities() -> ClientCapabilities {
    ClientCapabilities {
        workspace: Some(WorkspaceClientCapabilities {
            did_change_watched_files: Option::from(DidChangeWatchedFilesClientCapabilities {
                dynamic_registration: Some(true),
                relative_pattern_support: Some(true),
            }),
            ..Default::default()
        }),
        window: Some(WindowClientCapabilities {
            work_done_progress: Some(true),
            ..Default::default()
        }),
        text_document: Some(TextDocumentClientCapabilities {
            synchronization: Some(TextDocumentSyncClientCapabilities {
                dynamic_registration: Some(true),
                will_save: Some(false),
                will_save_wait_until: Some(false),
                did_save: Some(true),
            }),
            definition: Some(GotoCapability {
                dynamic_registration: Some(false),
                link_support: Some(true),
            }),
            hover: Some(HoverClientCapabilities {
                dynamic_registration: Some(false),
                // Request Markdown content from the LSP for hover responses.
                // This enables proper syntax highlighting in hover tooltips.
                content_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
            }),
            publish_diagnostics: Some(PublishDiagnosticsClientCapabilities {
                version_support: Some(true),
                related_information: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

pub fn default_init_params(workspace_uri: &Path, client_name: String) -> Result<InitializeParams> {
    let workspace_folder = path_to_workspace_folder(workspace_uri)?;

    Ok(InitializeParams {
        process_id: Some(std::process::id()),
        capabilities: default_client_capabilities(),
        workspace_folders: Some(vec![workspace_folder]),
        client_info: Some(ClientInfo {
            name: client_name,
            version: option_env!("GIT_RELEASE_TAG").map(|s| s.to_string()),
        }),
        locale: None,
        work_done_progress_params: WorkDoneProgressParams::default(),
        ..Default::default()
    })
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
