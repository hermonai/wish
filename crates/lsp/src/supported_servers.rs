use std::sync::Arc;

use crate::servers::clangd::ClangdCandidate;
use crate::servers::generic::GenericLspCandidate;
use crate::servers::go::GoPlsCandidate;
use crate::servers::pyright::PyrightCandidate;
use crate::servers::rust::RustAnalyzerCandidate;
use crate::servers::typescript_language_server::TypeScriptLanguageServerCandidate;
#[cfg(not(target_arch = "wasm32"))]
use crate::CommandBuilder;
use crate::{LanguageId, LanguageServerCandidate};
#[cfg(not(target_arch = "wasm32"))]
use command::r#async::Command;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

/// Configuration for a custom LSP binary installation.
///
/// For most LSP servers, we just need the binary path. However, for Node.js-based
/// servers like Pyright, we need to run `node langserver.index.js --stdio` instead
/// of relying on the wrapper script (which has a shebang that requires node in PATH).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub struct CustomBinaryConfig {
    /// The path to the executable (e.g., node binary or rust-analyzer binary)
    pub binary_path: PathBuf,
    /// Additional arguments to pass before any server-specific args (e.g., the JS file path)
    pub prepend_args: Vec<String>,
}

/// Represents the different types of LSP servers supported by Wish.
///
/// This is also used in underlying sqlite type persistence. We should be careful
/// not to rename an existing variant, as it will break persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter)]
pub enum LSPServerType {
    // ── Original five (stable, do NOT rename) ───────────────────
    RustAnalyzer,
    GoPls,
    Pyright,
    TypeScriptLanguageServer,
    Clangd,

    // ── Systems / compiled ──────────────────────────────────────
    Zls,
    NimLangServer,
    SourceKitLsp,
    DartAnalysisServer,

    // ── JVM family ──────────────────────────────────────────────
    Jdtls,
    KotlinLanguageServer,
    Metals,

    // ── .NET family ─────────────────────────────────────────────
    OmniSharp,
    FsAutoComplete,

    // ── Scripting / dynamic ─────────────────────────────────────
    Solargraph,
    Intelephense,
    PerlNavigator,
    LuaLanguageServer,
    BashLanguageServer,

    // ── JS framework servers ────────────────────────────────────
    SvelteLanguageServer,
    VueLanguageServer,

    // ── Functional / BEAM ───────────────────────────────────────
    ElixirLs,
    ErlangLs,
    Hls,
    OcamlLsp,

    // ── Web markup / style ──────────────────────────────────────
    HtmlLanguageServer,
    CssLanguageServer,

    // ── Data / config ───────────────────────────────────────────
    JsonLanguageServer,
    YamlLanguageServer,
    Taplo,
    Sqls,
    GraphQLLs,
    Lemminx,

    // ── Documentation / academic ────────────────────────────────
    Marksman,
    Texlab,

    // ── Blockchain ──────────────────────────────────────────────
    SolidityLanguageServer,

    // ── DevOps / infra ──────────────────────────────────────────
    TerraformLs,
    DockerfileLs,
    CmakeLanguageServer,
}

/// Provides server-specific configuration for each LSP server type.
impl LSPServerType {
    /// Creates a properly configured Command for this LSP server type.
    ///
    /// Uses `CommandBuilder` to create the command, which ensures `.cmd`/`.bat`
    /// scripts are resolved on Windows and PATH is set correctly.
    ///
    /// If a custom binary config is provided (e.g., from our data_dir installation),
    /// it will be used. Otherwise, falls back to the system PATH.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn create_command(
        &self,
        custom_config: Option<CustomBinaryConfig>,
        executor: &CommandBuilder,
    ) -> Command {
        if let Some(config) = custom_config {
            let mut command = executor.command(&config.binary_path);
            command.args(&config.prepend_args);
            command.args(self.custom_install_args());
            command
        } else {
            let mut command = executor.command(self.binary_name());
            command.args(self.args());
            command
        }
    }

    /// Finds the configuration for a custom-installed binary in the data directory.
    ///
    /// This checks our custom installation location (`{data_dir}/{server_name}/`).
    /// For Node.js-based servers, this returns the node binary path plus the JS file as args.
    ///
    /// # Arguments
    /// * `path_env_var` - The PATH environment variable to use when checking for system dependencies
    ///   (e.g., system node for pyright).
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn find_installed_binary_config(
        &self,
        path_env_var: Option<&str>,
    ) -> Option<CustomBinaryConfig> {
        match self {
            LSPServerType::RustAnalyzer => {
                RustAnalyzerCandidate::find_installed_binary_in_data_dir()
                    .await
                    .map(|path| CustomBinaryConfig {
                        binary_path: path,
                        prepend_args: vec![],
                    })
            }
            LSPServerType::GoPls => {
                // gopls doesn't support custom installation yet
                None
            }
            LSPServerType::Pyright => {
                PyrightCandidate::find_installed_binary_config(path_env_var).await
            }
            LSPServerType::TypeScriptLanguageServer => {
                TypeScriptLanguageServerCandidate::find_installed_binary_config(path_env_var).await
            }
            LSPServerType::Clangd => ClangdCandidate::find_installed_binary_in_data_dir()
                .await
                .map(|path| CustomBinaryConfig {
                    binary_path: path,
                    prepend_args: vec![],
                }),

            // All generic servers — no custom data_dir installation.
            _ => None,
        }
    }

    /// Checks if the binary works on the given PATH by running a version/help command.
    ///
    /// Delegates to each server's candidate implementation.
    /// Returns true only if the binary executes successfully with exit code 0.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn is_working_on_path(
        &self,
        executor: &CommandBuilder,
        client: Arc<http_client::Client>,
    ) -> bool {
        self.candidate(client).is_installed_on_path(executor).await
    }

    pub fn binary_name(&self) -> &'static str {
        match self {
            // ── Original five ───────────────────────────────────
            LSPServerType::RustAnalyzer => "rust-analyzer",
            LSPServerType::GoPls => "gopls",
            LSPServerType::Pyright => "pyright-langserver",
            LSPServerType::TypeScriptLanguageServer => "typescript-language-server",
            LSPServerType::Clangd => "clangd",

            // ── Systems / compiled ──────────────────────────────
            LSPServerType::Zls => "zls",
            LSPServerType::NimLangServer => "nimlangserver",
            LSPServerType::SourceKitLsp => "sourcekit-lsp",
            LSPServerType::DartAnalysisServer => "dart",

            // ── JVM family ──────────────────────────────────────
            LSPServerType::Jdtls => "jdtls",
            LSPServerType::KotlinLanguageServer => "kotlin-language-server",
            LSPServerType::Metals => "metals",

            // ── .NET family ─────────────────────────────────────
            LSPServerType::OmniSharp => "OmniSharp",
            LSPServerType::FsAutoComplete => "fsautocomplete",

            // ── Scripting / dynamic ─────────────────────────────
            LSPServerType::Solargraph => "solargraph",
            LSPServerType::Intelephense => "intelephense",
            LSPServerType::PerlNavigator => "perlnavigator",
            LSPServerType::LuaLanguageServer => "lua-language-server",
            LSPServerType::BashLanguageServer => "bash-language-server",

            // ── JS framework servers ────────────────────────────
            LSPServerType::SvelteLanguageServer => "svelteserver",
            LSPServerType::VueLanguageServer => "vue-language-server",

            // ── Functional / BEAM ───────────────────────────────
            LSPServerType::ElixirLs => "elixir-ls",
            LSPServerType::ErlangLs => "erlang_ls",
            LSPServerType::Hls => "haskell-language-server-wrapper",
            LSPServerType::OcamlLsp => "ocamllsp",

            // ── Web markup / style ──────────────────────────────
            LSPServerType::HtmlLanguageServer => "vscode-html-language-server",
            LSPServerType::CssLanguageServer => "vscode-css-language-server",

            // ── Data / config ───────────────────────────────────
            LSPServerType::JsonLanguageServer => "vscode-json-language-server",
            LSPServerType::YamlLanguageServer => "yaml-language-server",
            LSPServerType::Taplo => "taplo",
            LSPServerType::Sqls => "sqls",
            LSPServerType::GraphQLLs => "graphql-lsp",
            LSPServerType::Lemminx => "lemminx",

            // ── Documentation / academic ────────────────────────
            LSPServerType::Marksman => "marksman",
            LSPServerType::Texlab => "texlab",

            // ── Blockchain ──────────────────────────────────────
            LSPServerType::SolidityLanguageServer => "nomicfoundation-solidity-language-server",

            // ── DevOps / infra ──────────────────────────────────
            LSPServerType::TerraformLs => "terraform-ls",
            LSPServerType::DockerfileLs => "docker-langserver",
            LSPServerType::CmakeLanguageServer => "cmake-language-server",
        }
    }

    /// Arguments for running via system PATH.
    #[cfg(not(target_arch = "wasm32"))]
    fn args(&self) -> Vec<&'static str> {
        match self {
            // ── No extra args (server uses stdio by default) ────
            LSPServerType::RustAnalyzer
            | LSPServerType::GoPls
            | LSPServerType::Clangd
            | LSPServerType::Zls
            | LSPServerType::NimLangServer
            | LSPServerType::SourceKitLsp
            | LSPServerType::Jdtls
            | LSPServerType::KotlinLanguageServer
            | LSPServerType::Metals
            | LSPServerType::ElixirLs
            | LSPServerType::ErlangLs
            | LSPServerType::OcamlLsp
            | LSPServerType::Sqls
            | LSPServerType::Lemminx
            | LSPServerType::Texlab
            | LSPServerType::CmakeLanguageServer => vec![],

            // ── --stdio ─────────────────────────────────────────
            LSPServerType::Pyright
            | LSPServerType::TypeScriptLanguageServer
            | LSPServerType::Intelephense
            | LSPServerType::FsAutoComplete
            | LSPServerType::LuaLanguageServer
            | LSPServerType::PerlNavigator
            | LSPServerType::HtmlLanguageServer
            | LSPServerType::CssLanguageServer
            | LSPServerType::JsonLanguageServer
            | LSPServerType::YamlLanguageServer
            | LSPServerType::SvelteLanguageServer
            | LSPServerType::VueLanguageServer
            | LSPServerType::DockerfileLs
            | LSPServerType::SolidityLanguageServer => vec!["--stdio"],

            // ── Special args ────────────────────────────────────
            LSPServerType::Solargraph => vec!["stdio"],
            LSPServerType::OmniSharp => vec!["-lsp"],
            LSPServerType::DartAnalysisServer => vec!["language-server", "--protocol=lsp"],
            LSPServerType::Hls => vec!["--lsp"],
            LSPServerType::BashLanguageServer => vec!["start"],
            LSPServerType::Taplo => vec!["lsp", "stdio"],
            LSPServerType::GraphQLLs => vec!["server", "-m", "stream"],
            LSPServerType::Marksman => vec!["server"],
            LSPServerType::TerraformLs => vec!["serve"],
        }
    }

    /// Arguments for running from a custom installation.
    /// These are added after any prepend_args from CustomBinaryConfig.
    #[cfg(not(target_arch = "wasm32"))]
    fn custom_install_args(&self) -> Vec<&'static str> {
        // For the original five servers, custom install args may differ from
        // PATH args. All generic servers share the same args for both paths
        // since they don't support custom installation.
        match self {
            LSPServerType::RustAnalyzer => vec![],
            LSPServerType::GoPls => vec![],
            LSPServerType::Pyright => vec!["--stdio"],
            LSPServerType::TypeScriptLanguageServer => vec!["--stdio"],
            LSPServerType::Clangd => vec![],
            // All others: same as PATH args
            _ => self.args(),
        }
    }

    /// Returns the languages supported by this LSP server.
    pub fn languages(&self) -> Vec<LanguageId> {
        match self {
            // ── Original five ───────────────────────────────────
            LSPServerType::RustAnalyzer => vec![LanguageId::Rust],
            LSPServerType::GoPls => vec![LanguageId::Go],
            LSPServerType::Pyright => vec![LanguageId::Python],
            LSPServerType::TypeScriptLanguageServer => {
                vec![
                    LanguageId::TypeScript,
                    LanguageId::TypeScriptReact,
                    LanguageId::JavaScript,
                    LanguageId::JavaScriptReact,
                ]
            }
            LSPServerType::Clangd => vec![LanguageId::C, LanguageId::Cpp],

            // ── Systems / compiled ──────────────────────────────
            LSPServerType::Zls => vec![LanguageId::Zig],
            LSPServerType::NimLangServer => vec![LanguageId::Nim],
            LSPServerType::SourceKitLsp => vec![LanguageId::Swift],
            LSPServerType::DartAnalysisServer => vec![LanguageId::Dart],

            // ── JVM family ──────────────────────────────────────
            LSPServerType::Jdtls => vec![LanguageId::Java],
            LSPServerType::KotlinLanguageServer => vec![LanguageId::Kotlin],
            LSPServerType::Metals => vec![LanguageId::Scala],

            // ── .NET family ─────────────────────────────────────
            LSPServerType::OmniSharp => vec![LanguageId::CSharp],
            LSPServerType::FsAutoComplete => vec![LanguageId::FSharp],

            // ── Scripting / dynamic ─────────────────────────────
            LSPServerType::Solargraph => vec![LanguageId::Ruby],
            LSPServerType::Intelephense => vec![LanguageId::PHP],
            LSPServerType::PerlNavigator => vec![LanguageId::Perl],
            LSPServerType::LuaLanguageServer => vec![LanguageId::Lua],
            LSPServerType::BashLanguageServer => vec![LanguageId::Bash],

            // ── JS framework servers ────────────────────────────
            LSPServerType::SvelteLanguageServer => vec![LanguageId::Svelte],
            LSPServerType::VueLanguageServer => vec![LanguageId::Vue],

            // ── Functional / BEAM ───────────────────────────────
            LSPServerType::ElixirLs => vec![LanguageId::Elixir],
            LSPServerType::ErlangLs => vec![LanguageId::Erlang],
            LSPServerType::Hls => vec![LanguageId::Haskell],
            LSPServerType::OcamlLsp => vec![LanguageId::OCaml],

            // ── Web markup / style ──────────────────────────────
            LSPServerType::HtmlLanguageServer => vec![LanguageId::Html],
            LSPServerType::CssLanguageServer => {
                vec![LanguageId::Css, LanguageId::Scss, LanguageId::Less]
            }

            // ── Data / config ───────────────────────────────────
            LSPServerType::JsonLanguageServer => vec![LanguageId::Json, LanguageId::Jsonc],
            LSPServerType::YamlLanguageServer => vec![LanguageId::Yaml],
            LSPServerType::Taplo => vec![LanguageId::Toml],
            LSPServerType::Sqls => vec![LanguageId::Sql],
            LSPServerType::GraphQLLs => vec![LanguageId::GraphQL],
            LSPServerType::Lemminx => vec![LanguageId::Xml],

            // ── Documentation / academic ────────────────────────
            LSPServerType::Marksman => vec![LanguageId::Markdown],
            LSPServerType::Texlab => vec![LanguageId::LaTeX],

            // ── Blockchain ──────────────────────────────────────
            LSPServerType::SolidityLanguageServer => vec![LanguageId::Solidity],

            // ── DevOps / infra ──────────────────────────────────
            LSPServerType::TerraformLs => vec![LanguageId::Terraform],
            LSPServerType::DockerfileLs => vec![LanguageId::Dockerfile],
            LSPServerType::CmakeLanguageServer => vec![LanguageId::CMake],
        }
    }

    /// Returns a display name for the languages supported by this server.
    /// For multi-language servers, returns "Language1/Language2".
    pub fn language_name(&self) -> String {
        match self {
            LSPServerType::TypeScriptLanguageServer => "TypeScript/JavaScript".to_string(),
            LSPServerType::CssLanguageServer => "CSS/SCSS/Less".to_string(),
            LSPServerType::JsonLanguageServer => "JSON".to_string(),
            LSPServerType::Clangd => "C/C++".to_string(),
            _ => self
                .languages()
                .iter()
                .map(|lang| {
                    let id = lang.lsp_language_identifier();
                    let mut chars = id.chars();
                    // Capitalize the first character.
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    }
                })
                .join("/"),
        }
    }

    pub fn candidate(&self, client: Arc<http_client::Client>) -> Box<dyn LanguageServerCandidate> {
        match self {
            // ── Original five — specialised candidates ──────────
            LSPServerType::RustAnalyzer => Box::new(RustAnalyzerCandidate::new(client)),
            LSPServerType::GoPls => Box::new(GoPlsCandidate::new(client)),
            LSPServerType::Pyright => Box::new(PyrightCandidate::new(client)),
            LSPServerType::TypeScriptLanguageServer => {
                Box::new(TypeScriptLanguageServerCandidate::new(client))
            }
            LSPServerType::Clangd => Box::new(ClangdCandidate::new(client)),

            // ── Systems / compiled ──────────────────────────────
            LSPServerType::Zls => Box::new(GenericLspCandidate::new(
                client,
                "zls",
                &["--version"],
                &["build.zig", "build.zig.zon"],
                &["zig"],
            )),
            LSPServerType::NimLangServer => Box::new(GenericLspCandidate::new(
                client,
                "nimlangserver",
                &["--version"],
                &[],
                &["nim", "nims", "nimble"],
            )),
            LSPServerType::SourceKitLsp => Box::new(GenericLspCandidate::new(
                client,
                "sourcekit-lsp",
                &["--help"],
                &["Package.swift"],
                &["swift"],
            )),
            LSPServerType::DartAnalysisServer => Box::new(GenericLspCandidate::new(
                client,
                "dart",
                &["--version"],
                &["pubspec.yaml"],
                &["dart"],
            )),

            // ── JVM family ──────────────────────────────────────
            LSPServerType::Jdtls => Box::new(GenericLspCandidate::new(
                client,
                "jdtls",
                &["--version"],
                &["pom.xml", "build.gradle", "build.gradle.kts", ".classpath"],
                &["java"],
            )),
            LSPServerType::KotlinLanguageServer => Box::new(GenericLspCandidate::new(
                client,
                "kotlin-language-server",
                &["--version"],
                &["build.gradle.kts", "build.gradle"],
                &["kt", "kts"],
            )),
            LSPServerType::Metals => Box::new(GenericLspCandidate::new(
                client,
                "metals",
                &["--version"],
                &["build.sbt", "build.sc"],
                &["scala", "sc"],
            )),

            // ── .NET family ─────────────────────────────────────
            LSPServerType::OmniSharp => Box::new(GenericLspCandidate::new(
                client,
                "OmniSharp",
                &["--version"],
                &[],
                &["cs"],
            )),
            LSPServerType::FsAutoComplete => Box::new(GenericLspCandidate::new(
                client,
                "fsautocomplete",
                &["--version"],
                &[],
                &["fs", "fsx", "fsi"],
            )),

            // ── Scripting / dynamic ─────────────────────────────
            LSPServerType::Solargraph => Box::new(GenericLspCandidate::new(
                client,
                "solargraph",
                &["--version"],
                &["Gemfile", ".ruby-version"],
                &["rb"],
            )),
            LSPServerType::Intelephense => Box::new(GenericLspCandidate::new(
                client,
                "intelephense",
                &["--version"],
                &["composer.json"],
                &["php"],
            )),
            LSPServerType::PerlNavigator => Box::new(GenericLspCandidate::new(
                client,
                "perlnavigator",
                &["--version"],
                &["Makefile.PL", "Build.PL", "cpanfile"],
                &["pl", "pm"],
            )),
            LSPServerType::LuaLanguageServer => Box::new(GenericLspCandidate::new(
                client,
                "lua-language-server",
                &["--version"],
                &[".luarc.json", ".luacheckrc"],
                &["lua"],
            )),
            LSPServerType::BashLanguageServer => Box::new(GenericLspCandidate::new(
                client,
                "bash-language-server",
                &["--version"],
                &[],
                &["sh", "bash", "zsh"],
            )),

            // ── JS framework servers ────────────────────────────
            LSPServerType::SvelteLanguageServer => Box::new(GenericLspCandidate::new(
                client,
                "svelteserver",
                &["--version"],
                &["svelte.config.js", "svelte.config.ts"],
                &["svelte"],
            )),
            LSPServerType::VueLanguageServer => Box::new(GenericLspCandidate::new(
                client,
                "vue-language-server",
                &["--version"],
                &["vue.config.js", "vite.config.ts", "vite.config.js"],
                &["vue"],
            )),

            // ── Functional / BEAM ───────────────────────────────
            LSPServerType::ElixirLs => Box::new(GenericLspCandidate::new(
                client,
                "elixir-ls",
                &["--version"],
                &["mix.exs"],
                &["ex", "exs"],
            )),
            LSPServerType::ErlangLs => Box::new(GenericLspCandidate::new(
                client,
                "erlang_ls",
                &["--version"],
                &["rebar.config", "erlang.mk"],
                &["erl", "hrl"],
            )),
            LSPServerType::Hls => Box::new(GenericLspCandidate::new(
                client,
                "haskell-language-server-wrapper",
                &["--version"],
                &["stack.yaml", "cabal.project"],
                &["hs", "lhs"],
            )),
            LSPServerType::OcamlLsp => Box::new(GenericLspCandidate::new(
                client,
                "ocamllsp",
                &["--version"],
                &["dune-project"],
                &["ml", "mli"],
            )),

            // ── Web markup / style ──────────────────────────────
            LSPServerType::HtmlLanguageServer => Box::new(GenericLspCandidate::new(
                client,
                "vscode-html-language-server",
                &["--version"],
                &[],
                &["html", "htm"],
            )),
            LSPServerType::CssLanguageServer => Box::new(GenericLspCandidate::new(
                client,
                "vscode-css-language-server",
                &["--version"],
                &[],
                &["css", "scss", "less"],
            )),

            // ── Data / config ───────────────────────────────────
            LSPServerType::JsonLanguageServer => Box::new(GenericLspCandidate::new(
                client,
                "vscode-json-language-server",
                &["--version"],
                &[],
                &["json", "jsonc"],
            )),
            LSPServerType::YamlLanguageServer => Box::new(GenericLspCandidate::new(
                client,
                "yaml-language-server",
                &["--version"],
                &[],
                &["yaml", "yml"],
            )),
            LSPServerType::Taplo => Box::new(GenericLspCandidate::new(
                client,
                "taplo",
                &["--version"],
                &[],
                &["toml"],
            )),
            LSPServerType::Sqls => Box::new(GenericLspCandidate::new(
                client,
                "sqls",
                &["--version"],
                &[],
                &["sql"],
            )),
            LSPServerType::GraphQLLs => Box::new(GenericLspCandidate::new(
                client,
                "graphql-lsp",
                &["--version"],
                &[".graphqlrc", ".graphqlconfig", ".graphqlrc.yml"],
                &["graphql", "gql"],
            )),
            LSPServerType::Lemminx => Box::new(GenericLspCandidate::new(
                client,
                "lemminx",
                &["--version"],
                &[],
                &["xml", "xsl", "xsd"],
            )),

            // ── Documentation / academic ────────────────────────
            LSPServerType::Marksman => Box::new(GenericLspCandidate::new(
                client,
                "marksman",
                &["--version"],
                &[],
                &["md", "markdown"],
            )),
            LSPServerType::Texlab => Box::new(GenericLspCandidate::new(
                client,
                "texlab",
                &["--version"],
                &[],
                &["tex", "bib"],
            )),

            // ── Blockchain ──────────────────────────────────────
            LSPServerType::SolidityLanguageServer => Box::new(GenericLspCandidate::new(
                client,
                "nomicfoundation-solidity-language-server",
                &["--version"],
                &[
                    "hardhat.config.js",
                    "hardhat.config.ts",
                    "foundry.toml",
                    "truffle-config.js",
                    "brownie-config.yaml",
                ],
                &["sol"],
            )),

            // ── DevOps / infra ──────────────────────────────────
            LSPServerType::TerraformLs => Box::new(GenericLspCandidate::new(
                client,
                "terraform-ls",
                &["--version"],
                &[],
                &["tf", "tfvars"],
            )),
            LSPServerType::DockerfileLs => Box::new(GenericLspCandidate::new(
                client,
                "docker-langserver",
                &["--version"],
                &["Dockerfile", "dockerfile", "Containerfile"],
                &[],
            )),
            LSPServerType::CmakeLanguageServer => Box::new(GenericLspCandidate::new(
                client,
                "cmake-language-server",
                &["--version"],
                &["CMakeLists.txt"],
                &["cmake"],
            )),
        }
    }

    pub fn all() -> impl Iterator<Item = LSPServerType> {
        LSPServerType::iter()
    }
}
