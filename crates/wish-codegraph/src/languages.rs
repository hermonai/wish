//! Multi-language function + symbol extractors.
//!
//! The v0.5.0 family covers Rust, Python, TypeScript/JavaScript, and
//! Go via hand-rolled line-based parsers — no tree-sitter dep, no
//! grammars to bundle. They each catch the **shape** of a function or
//! class declaration without resolving call sites; that's enough to
//! build a codegraph dense enough to navigate.
//!
//! Each language returns the same `ParsedFn` shape so the rest of
//! `wish-codegraph` is language-agnostic. Real call-resolution lands
//! in v0.6.0 via tree-sitter (and the `syntax_tree` crate already in
//! the workspace).

use std::path::Path;

/// Source language detected from file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
    TypeScript,
    JavaScript,
    Go,
}

impl Language {
    /// Detect a language from a path's extension. Returns `None` for
    /// unknown extensions; the codegraph walker silently skips those.
    pub fn from_path(p: &Path) -> Option<Self> {
        let ext = p.extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "rs" => Self::Rust,
            "py" | "pyi" => Self::Python,
            "ts" | "tsx" => Self::TypeScript,
            "js" | "jsx" | "mjs" | "cjs" => Self::JavaScript,
            "go" => Self::Go,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
            Self::Go => "go",
        }
    }
}

/// Parsed function declaration. The fields are language-agnostic;
/// language-specific quirks (decorators, exports, receivers) collapse
/// into the booleans below.
#[derive(Debug, Clone)]
pub struct ParsedFn {
    pub name: String,
    pub line: u32,
    pub is_test: bool,
    pub is_pub: bool,
    pub is_async: bool,
}

/// Extract function declarations from a source file's text for the
/// given language. Returns an empty Vec if the language can't be
/// handled. Always finishes in linear time over the input — no
/// regex, no backtracking, no panic.
pub fn extract_functions(lang: Language, text: &str) -> Vec<ParsedFn> {
    match lang {
        Language::Rust => extract_rust(text),
        Language::Python => extract_python(text),
        Language::TypeScript | Language::JavaScript => extract_ts(text),
        Language::Go => extract_go(text),
    }
}

// -- Rust --------------------------------------------------------------

fn extract_rust(text: &str) -> Vec<ParsedFn> {
    let mut out = Vec::new();
    let mut prev_line: &str = "";
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim_start();
        if line.starts_with("//") {
            prev_line = raw.trim();
            continue;
        }
        if let Some(parsed) = parse_rust_signature(line, idx as u32, prev_line.trim()) {
            out.push(parsed);
        }
        prev_line = raw.trim();
    }
    out
}

fn parse_rust_signature(line: &str, line_no: u32, prev_line: &str) -> Option<ParsedFn> {
    let s = line.trim_start();
    let is_pub = s.starts_with("pub ") || s.starts_with("pub(");
    let mut rest = if is_pub {
        if s.starts_with("pub(") {
            let end = s.find(')')?;
            &s[end + 1..]
        } else {
            &s[3..]
        }
    } else {
        s
    };
    rest = rest.trim_start();
    let mut is_async = false;
    if let Some(stripped) = rest.strip_prefix("async ") {
        is_async = true;
        rest = stripped.trim_start();
    }
    for prefix in &["const ", "unsafe ", "extern \"C\" ", "extern "] {
        if let Some(stripped) = rest.strip_prefix(prefix) {
            rest = stripped.trim_start();
        }
    }
    let rest = rest.strip_prefix("fn ")?;
    let name_end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    let name = &rest[..name_end];
    if name.is_empty() {
        return None;
    }
    let after_name = rest[name_end..].trim_start();
    if !after_name.starts_with('(') && !after_name.starts_with('<') {
        return None;
    }
    let is_test = prev_line.contains("#[test]");
    Some(ParsedFn {
        name: name.to_string(),
        line: line_no + 1,
        is_test,
        is_pub,
        is_async,
    })
}

// -- Python ------------------------------------------------------------

fn extract_python(text: &str) -> Vec<ParsedFn> {
    let mut out = Vec::new();
    let mut last_was_test_decorator = false;
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim_start();
        // Detect `@pytest.mark.test`-ish decorators on the previous
        // non-empty line so we tag `is_test`.
        if line.starts_with('@') {
            if line.contains("test") || line.contains(".mark.") {
                last_was_test_decorator = true;
            }
            continue;
        }
        if let Some(parsed) = parse_python_signature(line, idx as u32, last_was_test_decorator) {
            out.push(parsed);
            last_was_test_decorator = false;
        } else if !line.is_empty() && !line.starts_with('#') {
            last_was_test_decorator = false;
        }
    }
    out
}

fn parse_python_signature(line: &str, line_no: u32, prev_test_decorator: bool) -> Option<ParsedFn> {
    let s = line.trim_start();
    let mut is_async = false;
    let mut rest = if let Some(stripped) = s.strip_prefix("async def ") {
        is_async = true;
        stripped
    } else if let Some(stripped) = s.strip_prefix("def ") {
        stripped
    } else if let Some(stripped) = s.strip_prefix("class ") {
        // Treat a class as a (non-async) function-shaped node so it
        // appears in the codegraph. Distinguished by `is_pub=true` and
        // a capitalized name. We don't have separate node kinds yet.
        stripped
    } else {
        return None;
    };
    let name_end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    let name = rest[..name_end].to_string();
    if name.is_empty() {
        return None;
    }
    rest = &rest[name_end..];
    // class A(Base): or class A: — must be `:` or `(` next.
    // def foo(...) -> Bar: — must be `(` next.
    let next = rest.trim_start();
    if !next.starts_with('(') && !next.starts_with(':') {
        return None;
    }
    // Python convention: a name starting with `_` is module-private,
    // anything else is publicly importable.
    let is_pub = !name.starts_with('_');
    let is_test = prev_test_decorator || name.starts_with("test_");
    Some(ParsedFn {
        name,
        line: line_no + 1,
        is_test,
        is_pub,
        is_async,
    })
}

// -- TypeScript / JavaScript -------------------------------------------

fn extract_ts(text: &str) -> Vec<ParsedFn> {
    let mut out = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim_start();
        if line.starts_with("//") {
            continue;
        }
        if let Some(parsed) = parse_ts_signature(line, idx as u32) {
            out.push(parsed);
        }
    }
    out
}

fn parse_ts_signature(line: &str, line_no: u32) -> Option<ParsedFn> {
    let mut s = line.trim_start();
    let mut is_pub = false;
    if let Some(stripped) = s.strip_prefix("export default ") {
        is_pub = true;
        s = stripped.trim_start();
    } else if let Some(stripped) = s.strip_prefix("export ") {
        is_pub = true;
        s = stripped.trim_start();
    }
    // TypeScript class member visibility keywords (best-effort).
    for vk in &["public ", "private ", "protected ", "static "] {
        if let Some(stripped) = s.strip_prefix(vk) {
            s = stripped.trim_start();
        }
    }
    let mut is_async = false;
    if let Some(stripped) = s.strip_prefix("async ") {
        is_async = true;
        s = stripped.trim_start();
    }
    // `function name(`
    if let Some(rest) = s.strip_prefix("function ") {
        let rest = rest.trim_start();
        let name_end = rest
            .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
            .unwrap_or(rest.len());
        let name = rest[..name_end].to_string();
        if !name.is_empty() {
            let is_test =
                name.starts_with("test") || name.starts_with("it") || name.starts_with("describe");
            return Some(ParsedFn {
                name,
                line: line_no + 1,
                is_test,
                is_pub: is_pub || !line.contains("private "),
                is_async,
            });
        }
    }
    // `class name`, `interface name`, `type name`
    for kw in &["class ", "interface ", "type "] {
        if let Some(rest) = s.strip_prefix(*kw) {
            let rest = rest.trim_start();
            let name_end = rest
                .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
                .unwrap_or(rest.len());
            let name = rest[..name_end].to_string();
            if !name.is_empty() {
                return Some(ParsedFn {
                    name,
                    line: line_no + 1,
                    is_test: false,
                    is_pub: true,
                    is_async: false,
                });
            }
        }
    }
    // `const name = (…) =>`, `let name = (…) =>`, `var name = (…) =>`
    for kw in &["const ", "let ", "var "] {
        if let Some(rest) = s.strip_prefix(*kw) {
            let rest = rest.trim_start();
            // Find name
            let name_end = rest
                .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
                .unwrap_or(rest.len());
            if name_end == 0 {
                continue;
            }
            let name = rest[..name_end].to_string();
            let after_name = rest[name_end..].trim_start();
            // We need `=` next, then eventually `=>` or `function`
            if !after_name.starts_with('=') {
                continue;
            }
            let after_eq = after_name[1..].trim_start();
            // The arrow function might be `() =>`, `(...) =>`, or even
            // `async () =>`. We only require the rest of the *line* to
            // contain `=>` or to start with `function`.
            let lower = after_eq;
            if lower.contains("=>") || lower.starts_with("function") || lower.starts_with("async") {
                let is_test = name.starts_with("test") || name.starts_with("it");
                return Some(ParsedFn {
                    name,
                    line: line_no + 1,
                    is_test,
                    is_pub,
                    is_async: is_async || lower.starts_with("async"),
                });
            }
        }
    }
    None
}

// -- Go ----------------------------------------------------------------

fn extract_go(text: &str) -> Vec<ParsedFn> {
    let mut out = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim_start();
        if line.starts_with("//") {
            continue;
        }
        if let Some(parsed) = parse_go_signature(line, idx as u32) {
            out.push(parsed);
        }
    }
    out
}

fn parse_go_signature(line: &str, line_no: u32) -> Option<ParsedFn> {
    let s = line.trim_start();
    if let Some(rest) = s.strip_prefix("func ") {
        let rest = rest.trim_start();
        // Methods: `func (r *T) Name(`
        let rest = if rest.starts_with('(') {
            // skip past the receiver clause
            let end = rest.find(')')?;
            rest[end + 1..].trim_start()
        } else {
            rest
        };
        let name_end = rest
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        let name = rest[..name_end].to_string();
        if name.is_empty() {
            return None;
        }
        // The name needs to be followed by '(' or generics '['.
        let after = rest[name_end..].trim_start();
        if !after.starts_with('(') && !after.starts_with('[') {
            return None;
        }
        // Go convention: an uppercase first letter means exported (pub).
        let is_pub = name
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false);
        let is_test = name.starts_with("Test") || name.starts_with("Benchmark");
        return Some(ParsedFn {
            name,
            line: line_no + 1,
            is_test,
            is_pub,
            is_async: false,
        });
    }
    // `type Name struct` / `type Name interface`
    if let Some(rest) = s.strip_prefix("type ") {
        let rest = rest.trim_start();
        let name_end = rest
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        let name = rest[..name_end].to_string();
        if name.is_empty() {
            return None;
        }
        let after = rest[name_end..].trim_start();
        if after.starts_with("struct") || after.starts_with("interface") {
            let is_pub = name
                .chars()
                .next()
                .map(|c| c.is_ascii_uppercase())
                .unwrap_or(false);
            return Some(ParsedFn {
                name,
                line: line_no + 1,
                is_test: false,
                is_pub,
                is_async: false,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_extractor_handles_visibility_async_test() {
        let src = "pub fn a() {}\nasync fn b() {}\n#[test]\nfn c() {}\n";
        let f = extract_functions(Language::Rust, src);
        assert_eq!(f.len(), 3);
        assert!(f[0].is_pub);
        assert!(f[1].is_async);
        assert!(f[2].is_test);
    }

    #[test]
    fn python_extractor_handles_def_class_async_and_test() {
        let src = "
def alpha():
    pass

async def beta(x):
    return x

class Gamma:
    def method(self):
        pass

def test_delta():
    assert True

def _private():
    pass

@pytest.mark.parametrize('x', [1,2])
def epsilon(x):
    pass
";
        let f = extract_functions(Language::Python, src);
        let names: Vec<&str> = f.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "alpha",
                "beta",
                "Gamma",
                "method",
                "test_delta",
                "_private",
                "epsilon"
            ]
        );

        let beta = f.iter().find(|p| p.name == "beta").unwrap();
        assert!(beta.is_async);
        let delta = f.iter().find(|p| p.name == "test_delta").unwrap();
        assert!(delta.is_test);
        let private = f.iter().find(|p| p.name == "_private").unwrap();
        assert!(!private.is_pub);
        let eps = f.iter().find(|p| p.name == "epsilon").unwrap();
        assert!(eps.is_test, "@pytest.mark decorator should set is_test");
    }

    #[test]
    fn ts_extractor_handles_function_arrow_class_interface_type() {
        let src = "
export function alpha() {}
export async function beta() {}
export const gamma = () => 1;
export const delta = async (x) => x;
class Epsilon {}
interface Zeta { x: number }
type Eta = { y: string };
let _internal = () => 0;
function test_thing() {}
";
        let f = extract_functions(Language::TypeScript, src);
        let names: Vec<&str> = f.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "alpha",
                "beta",
                "gamma",
                "delta",
                "Epsilon",
                "Zeta",
                "Eta",
                "_internal",
                "test_thing"
            ]
        );
        assert!(f.iter().find(|p| p.name == "beta").unwrap().is_async);
        assert!(f.iter().find(|p| p.name == "delta").unwrap().is_async);
        assert!(f.iter().find(|p| p.name == "test_thing").unwrap().is_test);
    }

    #[test]
    fn go_extractor_handles_func_method_struct_interface() {
        let src = "
package x

func Alpha() {}
func (r *T) Beta(x int) int { return x }
func gamma() {}
type Delta struct { x int }
type Epsilon interface { Read() }
func TestZeta(t *testing.T) {}
";
        let f = extract_functions(Language::Go, src);
        let names: Vec<&str> = f.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Alpha", "Beta", "gamma", "Delta", "Epsilon", "TestZeta"]
        );
        assert!(f.iter().find(|p| p.name == "Alpha").unwrap().is_pub);
        assert!(!f.iter().find(|p| p.name == "gamma").unwrap().is_pub);
        assert!(f.iter().find(|p| p.name == "TestZeta").unwrap().is_test);
    }

    #[test]
    fn language_from_path_covers_common_extensions() {
        assert_eq!(
            Language::from_path(std::path::Path::new("a.rs")),
            Some(Language::Rust)
        );
        assert_eq!(
            Language::from_path(std::path::Path::new("a.py")),
            Some(Language::Python)
        );
        assert_eq!(
            Language::from_path(std::path::Path::new("a.pyi")),
            Some(Language::Python)
        );
        assert_eq!(
            Language::from_path(std::path::Path::new("a.ts")),
            Some(Language::TypeScript)
        );
        assert_eq!(
            Language::from_path(std::path::Path::new("a.tsx")),
            Some(Language::TypeScript)
        );
        assert_eq!(
            Language::from_path(std::path::Path::new("a.js")),
            Some(Language::JavaScript)
        );
        assert_eq!(
            Language::from_path(std::path::Path::new("a.go")),
            Some(Language::Go)
        );
        assert_eq!(Language::from_path(std::path::Path::new("a.txt")), None);
    }
}
