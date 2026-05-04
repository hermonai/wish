//! Built-in SDLC (Software Development Lifecycle) agent definitions.
//!
//! These agents are pre-registered in the Hermon control plane and available
//! to all Wish users. They cover the complete development lifecycle from
//! planning through deployment and monitoring.

use super::agent::{
    AgentModelConfig, AgentParameters, AgentToolRef, AgentType, AgentVisibility,
    CreateAgentRequest,
};

/// All built-in SDLC agent slugs for programmatic access.
pub mod slugs {
    pub const PLANNER: &str = "wish-planner";
    pub const CODER: &str = "wish-coder";
    pub const REVIEWER: &str = "wish-reviewer";
    pub const TESTER: &str = "wish-tester";
    pub const DEBUGGER: &str = "wish-debugger";
    pub const DEPLOYER: &str = "wish-deployer";
    pub const DOCUMENTER: &str = "wish-documenter";
    pub const REFACTORER: &str = "wish-refactorer";
    pub const SECURITY: &str = "wish-security";
    pub const ORCHESTRATOR: &str = "wish-orchestrator";
}

/// Default model configuration for SDLC agents.
fn default_model() -> AgentModelConfig {
    AgentModelConfig {
        provider_id: "anthropic".into(),
        model_id: "claude-sonnet-4-6".into(),
        fallback_model_id: Some("claude-haiku-3-5".into()),
        temperature: Some(0.3),
        max_output_tokens: Some(8192),
    }
}

/// High-capability model for complex reasoning tasks.
fn reasoning_model() -> AgentModelConfig {
    AgentModelConfig {
        provider_id: "anthropic".into(),
        model_id: "claude-opus-4".into(),
        fallback_model_id: Some("claude-sonnet-4-6".into()),
        temperature: Some(0.2),
        max_output_tokens: Some(16384),
    }
}

/// Fast model for quick tasks.
fn fast_model() -> AgentModelConfig {
    AgentModelConfig {
        provider_id: "anthropic".into(),
        model_id: "claude-haiku-3-5".into(),
        fallback_model_id: None,
        temperature: Some(0.3),
        max_output_tokens: Some(4096),
    }
}

fn tool_ref(id: &str) -> AgentToolRef {
    AgentToolRef {
        tool_id: id.into(),
        config: None,
        requires_approval: false,
    }
}

fn tool_ref_with_approval(id: &str) -> AgentToolRef {
    AgentToolRef {
        tool_id: id.into(),
        config: None,
        requires_approval: true,
    }
}

/// Returns all built-in SDLC agent definitions.
pub fn builtin_agents() -> Vec<CreateAgentRequest> {
    vec![
        // ── Planner ──────────────────────────────────────────
        CreateAgentRequest {
            name: "Wish Planner".into(),
            slug: slugs::PLANNER.into(),
            description: Some(
                "Breaks down tasks into actionable implementation plans. \
                 Analyzes requirements, identifies dependencies, estimates effort, \
                 and produces step-by-step plans with file-level specificity."
                    .into(),
            ),
            model: reasoning_model(),
            tools: Some(vec![
                tool_ref("file_read"),
                tool_ref("file_search"),
                tool_ref("grep"),
                tool_ref("directory_tree"),
                tool_ref("git_log"),
                tool_ref("git_diff"),
            ]),
            system_prompt: Some(
                "You are Wish Planner, an expert software architect and project planner. \
                 Your role is to analyze codebases, understand requirements, and produce \
                 detailed implementation plans.\n\n\
                 When given a task:\n\
                 1. Analyze the existing codebase to understand the architecture\n\
                 2. Identify all files that need to be created or modified\n\
                 3. Determine dependencies and ordering constraints\n\
                 4. Estimate complexity for each step\n\
                 5. Produce a concrete, actionable plan with file paths\n\n\
                 Output plans in structured format with clear steps, file paths, \
                 and rationale for each change."
                    .into(),
            ),
            instructions: None,
            agent_type: AgentType::Sdlc,
            capabilities: Some(vec![
                "planning".into(),
                "architecture".into(),
                "codebase_analysis".into(),
                "task_decomposition".into(),
            ]),
            max_turns: Some(5),
            parameters: Some(AgentParameters {
                max_context_tokens: Some(150000),
                timeout_seconds: Some(120),
                retry_on_error: Some(true),
                parallel_tool_calls: Some(true),
            }),
            metadata: None,
            visibility: Some(AgentVisibility::System),
        },
        // ── Coder ────────────────────────────────────────────
        CreateAgentRequest {
            name: "Wish Coder".into(),
            slug: slugs::CODER.into(),
            description: Some(
                "Implements code changes based on plans or instructions. \
                 Reads existing code, writes new files, modifies existing ones, \
                 and runs commands to verify changes compile and pass basic checks."
                    .into(),
            ),
            model: default_model(),
            tools: Some(vec![
                tool_ref("file_read"),
                tool_ref("file_write"),
                tool_ref("file_edit"),
                tool_ref("file_search"),
                tool_ref("grep"),
                tool_ref("directory_tree"),
                tool_ref_with_approval("shell_exec"),
                tool_ref("git_diff"),
                tool_ref("git_status"),
            ]),
            system_prompt: Some(
                "You are Wish Coder, an expert software engineer. Your role is to \
                 implement code changes precisely and correctly.\n\n\
                 Guidelines:\n\
                 - Read relevant files before making changes\n\
                 - Follow existing code patterns and conventions\n\
                 - Write clean, well-documented code\n\
                 - Make minimal, focused changes\n\
                 - Verify changes compile after editing\n\
                 - Use the existing test infrastructure\n\
                 - Handle errors gracefully"
                    .into(),
            ),
            instructions: None,
            agent_type: AgentType::Coding,
            capabilities: Some(vec![
                "code_generation".into(),
                "code_editing".into(),
                "refactoring".into(),
                "bug_fixing".into(),
            ]),
            max_turns: Some(20),
            parameters: Some(AgentParameters {
                max_context_tokens: Some(150000),
                timeout_seconds: Some(300),
                retry_on_error: Some(true),
                parallel_tool_calls: Some(true),
            }),
            metadata: None,
            visibility: Some(AgentVisibility::System),
        },
        // ── Reviewer ─────────────────────────────────────────
        CreateAgentRequest {
            name: "Wish Reviewer".into(),
            slug: slugs::REVIEWER.into(),
            description: Some(
                "Reviews code changes for correctness, style, security, \
                 and best practices. Provides actionable feedback with \
                 specific line references and suggested fixes."
                    .into(),
            ),
            model: reasoning_model(),
            tools: Some(vec![
                tool_ref("file_read"),
                tool_ref("file_search"),
                tool_ref("grep"),
                tool_ref("git_diff"),
                tool_ref("git_log"),
            ]),
            system_prompt: Some(
                "You are Wish Reviewer, a senior code reviewer. Your role is to \
                 review code changes thoroughly and provide constructive feedback.\n\n\
                 Review checklist:\n\
                 1. Correctness — does the code do what it's supposed to?\n\
                 2. Edge cases — are boundary conditions handled?\n\
                 3. Error handling — are errors caught and handled gracefully?\n\
                 4. Security — any injection, auth, or data exposure risks?\n\
                 5. Performance — any obvious inefficiencies?\n\
                 6. Style — does it follow project conventions?\n\
                 7. Tests — are changes covered by tests?\n\n\
                 Provide specific, actionable feedback with file:line references."
                    .into(),
            ),
            instructions: None,
            agent_type: AgentType::Sdlc,
            capabilities: Some(vec![
                "code_review".into(),
                "security_audit".into(),
                "style_check".into(),
            ]),
            max_turns: Some(5),
            parameters: Some(AgentParameters {
                max_context_tokens: Some(200000),
                timeout_seconds: Some(120),
                retry_on_error: Some(true),
                parallel_tool_calls: Some(true),
            }),
            metadata: None,
            visibility: Some(AgentVisibility::System),
        },
        // ── Tester ───────────────────────────────────────────
        CreateAgentRequest {
            name: "Wish Tester".into(),
            slug: slugs::TESTER.into(),
            description: Some(
                "Generates and runs tests for code changes. Creates unit tests, \
                 integration tests, and test fixtures. Analyzes test coverage \
                 and suggests improvements."
                    .into(),
            ),
            model: default_model(),
            tools: Some(vec![
                tool_ref("file_read"),
                tool_ref("file_write"),
                tool_ref("file_edit"),
                tool_ref("file_search"),
                tool_ref("grep"),
                tool_ref_with_approval("shell_exec"),
                tool_ref("git_diff"),
            ]),
            system_prompt: Some(
                "You are Wish Tester, a testing expert. Your role is to write \
                 comprehensive tests and ensure code quality.\n\n\
                 Testing strategy:\n\
                 1. Identify testable units and integration points\n\
                 2. Write tests that cover happy paths and edge cases\n\
                 3. Use the project's existing test framework and patterns\n\
                 4. Create meaningful test names that document behavior\n\
                 5. Run tests and verify they pass\n\
                 6. Analyze coverage gaps and suggest additional tests\n\n\
                 Prefer property-based testing where appropriate."
                    .into(),
            ),
            instructions: None,
            agent_type: AgentType::Sdlc,
            capabilities: Some(vec![
                "test_generation".into(),
                "test_execution".into(),
                "coverage_analysis".into(),
            ]),
            max_turns: Some(15),
            parameters: Some(AgentParameters {
                max_context_tokens: Some(100000),
                timeout_seconds: Some(300),
                retry_on_error: Some(true),
                parallel_tool_calls: Some(false),
            }),
            metadata: None,
            visibility: Some(AgentVisibility::System),
        },
        // ── Debugger ─────────────────────────────────────────
        CreateAgentRequest {
            name: "Wish Debugger".into(),
            slug: slugs::DEBUGGER.into(),
            description: Some(
                "Diagnoses and fixes bugs. Analyzes error messages, stack traces, \
                 and logs to identify root causes. Proposes and implements fixes \
                 with verification."
                    .into(),
            ),
            model: reasoning_model(),
            tools: Some(vec![
                tool_ref("file_read"),
                tool_ref("file_edit"),
                tool_ref("file_search"),
                tool_ref("grep"),
                tool_ref_with_approval("shell_exec"),
                tool_ref("git_diff"),
                tool_ref("git_log"),
                tool_ref("git_blame"),
            ]),
            system_prompt: Some(
                "You are Wish Debugger, an expert at diagnosing and fixing software bugs.\n\n\
                 Debugging methodology:\n\
                 1. Reproduce — understand the error and its context\n\
                 2. Isolate — narrow down the root cause using logs, traces, code reading\n\
                 3. Diagnose — identify the specific bug and why it occurs\n\
                 4. Fix — implement a minimal, correct fix\n\
                 5. Verify — confirm the fix resolves the issue without regressions\n\n\
                 Always explain your reasoning step by step."
                    .into(),
            ),
            instructions: None,
            agent_type: AgentType::Sdlc,
            capabilities: Some(vec![
                "debugging".into(),
                "root_cause_analysis".into(),
                "error_diagnosis".into(),
            ]),
            max_turns: Some(20),
            parameters: Some(AgentParameters {
                max_context_tokens: Some(150000),
                timeout_seconds: Some(300),
                retry_on_error: Some(true),
                parallel_tool_calls: Some(true),
            }),
            metadata: None,
            visibility: Some(AgentVisibility::System),
        },
        // ── Deployer ─────────────────────────────────────────
        CreateAgentRequest {
            name: "Wish Deployer".into(),
            slug: slugs::DEPLOYER.into(),
            description: Some(
                "Manages deployment workflows. Creates and updates CI/CD configs, \
                 Dockerfiles, infrastructure-as-code, and deployment scripts. \
                 Monitors deployment status and rollback if needed."
                    .into(),
            ),
            model: default_model(),
            tools: Some(vec![
                tool_ref("file_read"),
                tool_ref("file_write"),
                tool_ref("file_edit"),
                tool_ref("file_search"),
                tool_ref("grep"),
                tool_ref_with_approval("shell_exec"),
                tool_ref("git_status"),
                tool_ref("git_log"),
            ]),
            system_prompt: Some(
                "You are Wish Deployer, a DevOps and deployment specialist.\n\n\
                 Your responsibilities:\n\
                 1. Configure CI/CD pipelines (GitHub Actions, GitLab CI, etc.)\n\
                 2. Write and optimize Dockerfiles\n\
                 3. Manage infrastructure-as-code (Terraform, Pulumi, etc.)\n\
                 4. Create deployment scripts with proper error handling\n\
                 5. Set up monitoring and alerting\n\
                 6. Implement blue-green/canary deployment strategies\n\n\
                 Always prioritize safety — use staged rollouts and include \
                 rollback procedures."
                    .into(),
            ),
            instructions: None,
            agent_type: AgentType::Sdlc,
            capabilities: Some(vec![
                "deployment".into(),
                "ci_cd".into(),
                "infrastructure".into(),
                "monitoring".into(),
            ]),
            max_turns: Some(15),
            parameters: Some(AgentParameters {
                max_context_tokens: Some(100000),
                timeout_seconds: Some(300),
                retry_on_error: Some(true),
                parallel_tool_calls: Some(false),
            }),
            metadata: None,
            visibility: Some(AgentVisibility::System),
        },
        // ── Documenter ───────────────────────────────────────
        CreateAgentRequest {
            name: "Wish Documenter".into(),
            slug: slugs::DOCUMENTER.into(),
            description: Some(
                "Generates and maintains documentation. Writes API docs, \
                 README files, architecture docs, inline comments, and \
                 changelog entries."
                    .into(),
            ),
            model: fast_model(),
            tools: Some(vec![
                tool_ref("file_read"),
                tool_ref("file_write"),
                tool_ref("file_edit"),
                tool_ref("file_search"),
                tool_ref("grep"),
                tool_ref("directory_tree"),
                tool_ref("git_log"),
                tool_ref("git_diff"),
            ]),
            system_prompt: Some(
                "You are Wish Documenter, a technical writing specialist.\n\n\
                 Documentation principles:\n\
                 1. Write for the reader — assume they're new to the codebase\n\
                 2. Include examples wherever possible\n\
                 3. Keep docs close to code (prefer inline/doc comments)\n\
                 4. Update existing docs when code changes\n\
                 5. Generate changelog entries from commits\n\
                 6. Follow the project's documentation conventions"
                    .into(),
            ),
            instructions: None,
            agent_type: AgentType::Sdlc,
            capabilities: Some(vec![
                "documentation".into(),
                "api_docs".into(),
                "changelog".into(),
            ]),
            max_turns: Some(10),
            parameters: Some(AgentParameters {
                max_context_tokens: Some(100000),
                timeout_seconds: Some(120),
                retry_on_error: Some(true),
                parallel_tool_calls: Some(true),
            }),
            metadata: None,
            visibility: Some(AgentVisibility::System),
        },
        // ── Refactorer ───────────────────────────────────────
        CreateAgentRequest {
            name: "Wish Refactorer".into(),
            slug: slugs::REFACTORER.into(),
            description: Some(
                "Improves code structure without changing behavior. Identifies \
                 code smells, suggests and implements refactoring patterns, \
                 and verifies behavior preservation through tests."
                    .into(),
            ),
            model: default_model(),
            tools: Some(vec![
                tool_ref("file_read"),
                tool_ref("file_write"),
                tool_ref("file_edit"),
                tool_ref("file_search"),
                tool_ref("grep"),
                tool_ref("directory_tree"),
                tool_ref_with_approval("shell_exec"),
                tool_ref("git_diff"),
            ]),
            system_prompt: Some(
                "You are Wish Refactorer, a code improvement specialist.\n\n\
                 Refactoring principles:\n\
                 1. Make behavior-preserving changes only\n\
                 2. Work in small, verifiable steps\n\
                 3. Run tests after each change\n\
                 4. Target specific code smells (duplication, long methods, etc.)\n\
                 5. Improve naming, structure, and abstractions\n\
                 6. Never change public APIs without explicit approval"
                    .into(),
            ),
            instructions: None,
            agent_type: AgentType::Sdlc,
            capabilities: Some(vec![
                "refactoring".into(),
                "code_quality".into(),
                "design_patterns".into(),
            ]),
            max_turns: Some(20),
            parameters: Some(AgentParameters {
                max_context_tokens: Some(150000),
                timeout_seconds: Some(300),
                retry_on_error: Some(true),
                parallel_tool_calls: Some(true),
            }),
            metadata: None,
            visibility: Some(AgentVisibility::System),
        },
        // ── Security ─────────────────────────────────────────
        CreateAgentRequest {
            name: "Wish Security".into(),
            slug: slugs::SECURITY.into(),
            description: Some(
                "Security-focused agent that audits code for vulnerabilities, \
                 checks dependencies for known CVEs, reviews auth flows, \
                 and ensures security best practices."
                    .into(),
            ),
            model: reasoning_model(),
            tools: Some(vec![
                tool_ref("file_read"),
                tool_ref("file_search"),
                tool_ref("grep"),
                tool_ref("directory_tree"),
                tool_ref("git_diff"),
                tool_ref("git_log"),
                tool_ref_with_approval("shell_exec"),
            ]),
            system_prompt: Some(
                "You are Wish Security, a cybersecurity expert.\n\n\
                 Security audit checklist:\n\
                 1. Input validation — check for injection vulnerabilities\n\
                 2. Authentication — verify auth flows are correct\n\
                 3. Authorization — check access control is enforced\n\
                 4. Secrets — scan for hardcoded credentials/keys\n\
                 5. Dependencies — check for known CVEs\n\
                 6. Data exposure — verify sensitive data isn't leaked\n\
                 7. Crypto — check for proper use of cryptographic primitives\n\
                 8. Error handling — ensure errors don't leak sensitive info\n\n\
                 Rate findings by severity: Critical, High, Medium, Low, Info."
                    .into(),
            ),
            instructions: None,
            agent_type: AgentType::Sdlc,
            capabilities: Some(vec![
                "security_audit".into(),
                "vulnerability_scan".into(),
                "dependency_check".into(),
            ]),
            max_turns: Some(10),
            parameters: Some(AgentParameters {
                max_context_tokens: Some(200000),
                timeout_seconds: Some(120),
                retry_on_error: Some(true),
                parallel_tool_calls: Some(true),
            }),
            metadata: None,
            visibility: Some(AgentVisibility::System),
        },
        // ── Orchestrator ─────────────────────────────────────
        CreateAgentRequest {
            name: "Wish Orchestrator".into(),
            slug: slugs::ORCHESTRATOR.into(),
            description: Some(
                "Meta-agent that coordinates other SDLC agents. Breaks down \
                 complex tasks, delegates to specialist agents, aggregates \
                 results, and manages the overall workflow."
                    .into(),
            ),
            model: reasoning_model(),
            tools: Some(vec![
                tool_ref("file_read"),
                tool_ref("file_search"),
                tool_ref("grep"),
                tool_ref("directory_tree"),
                tool_ref("agent_invoke"),
                tool_ref("agent_list"),
            ]),
            system_prompt: Some(
                "You are Wish Orchestrator, a project management AI that coordinates \
                 a team of specialist agents.\n\n\
                 Available agents:\n\
                 - wish-planner: Architecture and planning\n\
                 - wish-coder: Code implementation\n\
                 - wish-reviewer: Code review\n\
                 - wish-tester: Test generation and execution\n\
                 - wish-debugger: Bug diagnosis and fixes\n\
                 - wish-deployer: CI/CD and deployment\n\
                 - wish-documenter: Documentation\n\
                 - wish-refactorer: Code improvement\n\
                 - wish-security: Security auditing\n\n\
                 For complex tasks:\n\
                 1. Analyze the request and break it into sub-tasks\n\
                 2. Delegate each sub-task to the appropriate specialist\n\
                 3. Review results and iterate if needed\n\
                 4. Aggregate and present the final result"
                    .into(),
            ),
            instructions: None,
            agent_type: AgentType::Orchestrator,
            capabilities: Some(vec![
                "orchestration".into(),
                "delegation".into(),
                "workflow_management".into(),
            ]),
            max_turns: Some(30),
            parameters: Some(AgentParameters {
                max_context_tokens: Some(200000),
                timeout_seconds: Some(600),
                retry_on_error: Some(true),
                parallel_tool_calls: Some(true),
            }),
            metadata: None,
            visibility: Some(AgentVisibility::System),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_agents_count() {
        let agents = builtin_agents();
        assert_eq!(agents.len(), 10);
    }

    #[test]
    fn builtin_agents_unique_slugs() {
        let agents = builtin_agents();
        let mut seen = std::collections::HashSet::new();
        for a in &agents {
            assert!(seen.insert(&a.slug), "duplicate slug: {}", a.slug);
        }
    }

    #[test]
    fn builtin_agents_all_system_visibility() {
        let agents = builtin_agents();
        for a in &agents {
            assert_eq!(
                a.visibility,
                Some(AgentVisibility::System),
                "{} should be system visibility",
                a.slug
            );
        }
    }

    #[test]
    fn builtin_agents_have_tools() {
        let agents = builtin_agents();
        for a in &agents {
            assert!(
                a.tools.as_ref().map(|t| !t.is_empty()).unwrap_or(false),
                "{} should have tools",
                a.slug
            );
        }
    }

    #[test]
    fn builtin_agents_serialize() {
        let agents = builtin_agents();
        for a in &agents {
            let json = serde_json::to_value(&a).unwrap();
            assert!(json.get("name").is_some(), "{} missing name", a.slug);
            assert!(json.get("model").is_some(), "{} missing model", a.slug);
        }
    }

    #[test]
    fn orchestrator_is_orchestrator_type() {
        let agents = builtin_agents();
        let orch = agents.iter().find(|a| a.slug == slugs::ORCHESTRATOR).unwrap();
        assert_eq!(orch.agent_type, AgentType::Orchestrator);
    }
}
