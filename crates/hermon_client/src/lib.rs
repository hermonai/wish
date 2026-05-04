//! `hermon_client` — Typed Rust HTTP client for the Hermon control plane.
//!
//! Provides strongly-typed access to every Hermon API endpoint:
//! auth, AI model routing, sessions, orgs, telemetry.
//!
//! # Example
//!
//! ```no_run
//! use hermon_client::{HermonClient, HermonClientConfig, MemoryTokenStore};
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), hermon_client::HermonError> {
//! let client = HermonClient::new(
//!     HermonClientConfig::new("https://api.hermon.ai"),
//!     Arc::new(MemoryTokenStore::new()),
//! );
//!
//! // Register a device
//! use hermon_client::types::session::{DeviceRegistration, Platform};
//! let tokens = client.auth.register(DeviceRegistration {
//!     device_id: "dev-001".into(),
//!     name: "My Wish Terminal".into(),
//!     platform: Platform::Macos,
//!     client_version: "0.1.0".into(),
//! }).await?;
//!
//! // List AI models
//! let models = client.ai.list_models(None).await?;
//! # Ok(())
//! # }
//! ```

pub mod config;
pub mod error;
pub mod namespaces;
pub mod store;
pub mod transport;
pub mod types;

pub use config::{HermonClientConfig, RetryConfig};
pub use error::HermonError;
pub use store::{MemoryTokenStore, TokenStore};

use std::sync::Arc;

use namespaces::agents::AgentsNamespace;
use namespaces::ai::AiNamespace;
use namespaces::auth::AuthNamespace;
use namespaces::conversations::ConversationsNamespace;
use namespaces::drive::DriveNamespace;
use namespaces::orgs::OrgsNamespace;
use namespaces::sessions::SessionsNamespace;
use namespaces::telemetry::TelemetryNamespace;
use transport::HermonHttp;

/// The Hermon API client.
///
/// Organises endpoints into namespaces mirroring the TS client
/// (`@wish/hermon-client`). Each namespace holds an `Arc<HermonHttp>`
/// reference for zero-copy sharing.
pub struct HermonClient {
    pub auth: AuthNamespace,
    pub ai: AiNamespace,
    pub agents: AgentsNamespace,
    pub conversations: ConversationsNamespace,
    pub drive: DriveNamespace,
    pub sessions: SessionsNamespace,
    pub orgs: OrgsNamespace,
    pub telemetry: TelemetryNamespace,
    http: Arc<HermonHttp>,
}

impl HermonClient {
    /// Create a new client with the given config and token store.
    pub fn new(config: HermonClientConfig, token_store: Arc<dyn TokenStore>) -> Self {
        let http = Arc::new(HermonHttp::new(config, token_store));

        Self {
            auth: AuthNamespace {
                http: Arc::clone(&http),
            },
            ai: AiNamespace {
                http: Arc::clone(&http),
            },
            agents: AgentsNamespace {
                http: Arc::clone(&http),
            },
            conversations: ConversationsNamespace {
                http: Arc::clone(&http),
            },
            drive: DriveNamespace {
                http: Arc::clone(&http),
            },
            sessions: SessionsNamespace {
                http: Arc::clone(&http),
            },
            orgs: OrgsNamespace {
                http: Arc::clone(&http),
            },
            telemetry: TelemetryNamespace {
                http: Arc::clone(&http),
            },
            http,
        }
    }

    /// Health check — GET /healthz/live.
    pub async fn health(&self) -> Result<types::common::HealthResponse, HermonError> {
        self.http
            .request(transport::http::RequestOptions::get("/healthz/live").with_skip_auth(true))
            .await
    }

    /// Server version — GET /info/version.
    pub async fn version(&self) -> Result<types::common::VersionInfo, HermonError> {
        self.http
            .request(transport::http::RequestOptions::get("/info/version").with_skip_auth(true))
            .await
    }

    /// Access the underlying HTTP transport (advanced use).
    pub fn http(&self) -> &Arc<HermonHttp> {
        &self.http
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Error hierarchy ────────────────────────────────────────────

    #[test]
    fn error_display() {
        let e = HermonError::Auth {
            message: "bad token".into(),
            request_id: None,
        };
        assert_eq!(e.to_string(), "auth error: bad token");
    }

    #[test]
    fn error_retryable() {
        assert!(!HermonError::Auth {
            message: "x".into(),
            request_id: None,
        }
        .is_retryable());

        assert!(HermonError::Server {
            status: 500,
            message: "x".into(),
            request_id: None,
        }
        .is_retryable());

        assert!(HermonError::Network {
            message: "timeout".into(),
            source: None,
        }
        .is_retryable());

        assert!(HermonError::RateLimited {
            retry_after_ms: Some(1000),
            request_id: None,
        }
        .is_retryable());

        assert!(!HermonError::Validation {
            message: "bad".into(),
            detail: None,
            request_id: None,
        }
        .is_retryable());

        assert!(!HermonError::PolicyDenied {
            message: "denied".into(),
            code: None,
            request_id: None,
        }
        .is_retryable());
    }

    #[test]
    fn error_retry_after() {
        let e = HermonError::RateLimited {
            retry_after_ms: Some(5000),
            request_id: None,
        };
        assert_eq!(e.retry_after_ms(), Some(5000));

        let e2 = HermonError::Auth {
            message: "x".into(),
            request_id: None,
        };
        assert_eq!(e2.retry_after_ms(), None);
    }

    // ── Problem details → error mapping ────────────────────────────

    #[test]
    fn status_to_error_401() {
        let e = error::status_to_error(
            401,
            Some(error::ProblemDetails {
                r#type: None,
                title: Some("Unauthorized".into()),
                status: Some(401),
                detail: Some("Token expired".into()),
                request_id: Some("req-1".into()),
                code: None,
            }),
        );
        matches!(e, HermonError::Auth { .. });
    }

    #[test]
    fn status_to_error_403() {
        let e = error::status_to_error(403, None);
        matches!(e, HermonError::PolicyDenied { .. });
    }

    #[test]
    fn status_to_error_429() {
        let e = error::status_to_error(429, None);
        matches!(e, HermonError::RateLimited { .. });
    }

    #[test]
    fn status_to_error_400() {
        let e = error::status_to_error(
            400,
            Some(error::ProblemDetails {
                r#type: None,
                title: None,
                status: Some(400),
                detail: Some("invalid field".into()),
                request_id: None,
                code: None,
            }),
        );
        matches!(e, HermonError::Validation { .. });
    }

    #[test]
    fn status_to_error_500() {
        let e = error::status_to_error(500, None);
        matches!(e, HermonError::Server { status: 500, .. });
    }

    // ── Config ─────────────────────────────────────────────────────

    #[test]
    fn default_config() {
        let c = HermonClientConfig::default();
        assert_eq!(c.base_url, "http://localhost:9100");
        assert_eq!(c.retry.max_attempts, 3);
        assert_eq!(c.retry.base_ms, 200);
    }

    #[test]
    fn config_new() {
        let c = HermonClientConfig::new("https://api.hermon.ai");
        assert_eq!(c.base_url, "https://api.hermon.ai");
    }

    // ── MemoryTokenStore ───────────────────────────────────────────

    #[test]
    fn memory_store_empty() {
        let s = MemoryTokenStore::new();
        assert!(s.get_access().is_none());
        assert!(s.get_refresh().is_none());
        assert!(s.get_api_token().is_none());
    }

    #[test]
    fn memory_store_set_and_get() {
        let s = MemoryTokenStore::new();
        let future = chrono::Utc::now().timestamp_millis() + 60_000;
        s.set_tokens("access".into(), "refresh".into(), future);
        assert_eq!(s.get_access().unwrap(), "access");
        assert_eq!(s.get_refresh().unwrap(), "refresh");
    }

    #[test]
    fn memory_store_expired() {
        let s = MemoryTokenStore::new();
        let past = chrono::Utc::now().timestamp_millis() - 1000;
        s.set_tokens("access".into(), "refresh".into(), past);
        assert!(s.get_access().is_none()); // expired
        assert_eq!(s.get_refresh().unwrap(), "refresh"); // refresh never expires
    }

    #[test]
    fn memory_store_api_token() {
        let s = MemoryTokenStore::new();
        s.set_api_token("api-key-123".into());
        assert_eq!(s.get_api_token().unwrap(), "api-key-123");
    }

    #[test]
    fn memory_store_clear() {
        let s = MemoryTokenStore::new();
        let future = chrono::Utc::now().timestamp_millis() + 60_000;
        s.set_tokens("a".into(), "r".into(), future);
        s.set_api_token("k".into());
        s.clear();
        assert!(s.get_access().is_none());
        assert!(s.get_refresh().is_none());
        assert!(s.get_api_token().is_none());
    }

    // ── Wire type serde ────────────────────────────────────────────

    #[test]
    fn auth_tokens_deserialize() {
        let json = r#"{"accessToken":"at","refreshToken":"rt","expiresAt":1234567890}"#;
        let t: types::session::AuthTokens = serde_json::from_str(json).unwrap();
        assert_eq!(t.access_token, "at");
        assert_eq!(t.refresh_token, "rt");
        assert_eq!(t.expires_at, 1234567890);
    }

    #[test]
    fn session_deserialize() {
        let json = r#"{
            "id": "s1",
            "userId": "u1",
            "deviceId": "d1",
            "orgId": "o1",
            "createdAt": "2026-01-01T00:00:00Z",
            "expiresAt": "2026-01-02T00:00:00Z"
        }"#;
        let s: types::session::Session = serde_json::from_str(json).unwrap();
        assert_eq!(s.id, "s1");
        assert_eq!(s.user_id, "u1");
        assert_eq!(s.org_id, "o1");
    }

    #[test]
    fn user_deserialize() {
        let json = r#"{
            "id": "u1",
            "email": "test@hermon.ai",
            "displayName": "Test User",
            "orgId": "o1",
            "role": "admin",
            "createdAt": "2026-01-01T00:00:00Z"
        }"#;
        let u: types::session::User = serde_json::from_str(json).unwrap();
        assert_eq!(u.display_name, "Test User");
        assert_eq!(u.role, "admin");
    }

    #[test]
    fn device_registration_serializes_camel_case() {
        let reg = types::session::DeviceRegistration {
            device_id: "d1".into(),
            name: "Test".into(),
            platform: types::session::Platform::Macos,
            client_version: "0.1.0".into(),
        };
        let json = serde_json::to_value(&reg).unwrap();
        assert_eq!(json["deviceId"], "d1");
        assert_eq!(json["clientVersion"], "0.1.0");
        assert_eq!(json["platform"], "macos");
    }

    #[test]
    fn page_deserialize() {
        let json = r#"{
            "items": [{"id":"s1","userId":"u1","deviceId":"d1","orgId":"o1","createdAt":"t","expiresAt":"t"}],
            "total": 1,
            "offset": 0,
            "limit": 20,
            "hasMore": false
        }"#;
        let p: types::common::Page<types::session::Session> = serde_json::from_str(json).unwrap();
        assert_eq!(p.items.len(), 1);
        assert!(!p.has_more);
    }

    #[test]
    fn ai_model_deserialize() {
        let json = r#"{
            "id": "claude-sonnet-4-6",
            "providerId": "anthropic",
            "displayName": "Claude Sonnet 4.6",
            "contextWindow": 200000,
            "maxOutputTokens": 8192,
            "capabilities": ["chat", "tools"]
        }"#;
        let m: types::ai::AiModel = serde_json::from_str(json).unwrap();
        assert_eq!(m.id, "claude-sonnet-4-6");
        assert_eq!(m.provider_id, "anthropic");
        assert_eq!(m.capabilities.len(), 2);
    }

    #[test]
    fn ai_send_request_serializes() {
        let req = types::ai::AiSendRequest {
            session_id: "s1".into(),
            model: types::ai::ModelRef {
                provider_id: "anthropic".into(),
                model_id: "claude-sonnet-4-6".into(),
            },
            messages: vec![types::ai::AiMessage {
                id: "m1".into(),
                role: "user".into(),
                blocks: vec![serde_json::json!({"kind": "text", "text": "hello"})],
                created_at: "2026-01-01T00:00:00Z".into(),
            }],
            tools: None,
            parameters: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["sessionId"], "s1");
        assert_eq!(json["model"]["providerId"], "anthropic");
        assert_eq!(json["messages"][0]["role"], "user");
    }

    #[test]
    fn ai_stream_event_response_started() {
        let json = r#"{"kind":"response.started","responseId":"r1"}"#;
        let e: types::ai::AiStreamEvent = serde_json::from_str(json).unwrap();
        matches!(e, types::ai::AiStreamEvent::ResponseStarted { .. });
    }

    #[test]
    fn ai_stream_event_content_delta() {
        let json = r#"{"kind":"content.delta","delta":{"text":"hello"}}"#;
        let e: types::ai::AiStreamEvent = serde_json::from_str(json).unwrap();
        matches!(e, types::ai::AiStreamEvent::ContentDelta { .. });
    }

    #[test]
    fn ai_stream_event_error() {
        let json = r#"{"kind":"error","code":"rate_limit","message":"slow down"}"#;
        let e: types::ai::AiStreamEvent = serde_json::from_str(json).unwrap();
        matches!(e, types::ai::AiStreamEvent::Error { .. });
    }

    #[test]
    fn ai_stream_event_done() {
        let json = r#"{"kind":"done"}"#;
        let e: types::ai::AiStreamEvent = serde_json::from_str(json).unwrap();
        matches!(e, types::ai::AiStreamEvent::Done);
    }

    #[test]
    fn ai_usage_deserialize() {
        let json = r#"{"inputTokens":100,"outputTokens":50,"cachedInputTokens":10}"#;
        let u: types::ai::AiUsage = serde_json::from_str(json).unwrap();
        assert_eq!(u.input_tokens, 100);
        assert_eq!(u.output_tokens, 50);
        assert_eq!(u.cached_input_tokens, Some(10));
    }

    #[test]
    fn health_response_deserialize() {
        let json = r#"{"status":"ok","components":[{"name":"db","status":"healthy"}]}"#;
        let h: types::common::HealthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(h.status, "ok");
        assert_eq!(h.components[0].name, "db");
    }

    #[test]
    fn version_info_deserialize() {
        let json = r#"{"version":"0.1.0","gitSha":"abc123"}"#;
        let v: types::common::VersionInfo = serde_json::from_str(json).unwrap();
        assert_eq!(v.version, "0.1.0");
        assert_eq!(v.git_sha.unwrap(), "abc123");
    }

    #[test]
    fn problem_details_deserialize() {
        let json = r#"{"type":"about:blank","title":"Not Found","status":404,"detail":"session not found","requestId":"req-1"}"#;
        let p: error::ProblemDetails = serde_json::from_str(json).unwrap();
        assert_eq!(p.status, Some(404));
        assert_eq!(p.detail.unwrap(), "session not found");
        assert_eq!(p.request_id.unwrap(), "req-1");
    }

    #[test]
    fn problem_details_alias_request_id() {
        // Hermon sends both "request_id" and "requestId" depending on context
        let json = r#"{"request_id":"req-2"}"#;
        let p: error::ProblemDetails = serde_json::from_str(json).unwrap();
        assert_eq!(p.request_id.unwrap(), "req-2");
    }

    #[test]
    fn telemetry_batch_serializes() {
        let batch = types::ai::TelemetryBatch {
            events: vec![types::ai::TelemetryEvent {
                id: "e1".into(),
                event_type: "ai.request.completed".into(),
                ts: "2026-01-01T00:00:00Z".into(),
                level: "info".into(),
                attributes: None,
            }],
        };
        let json = serde_json::to_value(&batch).unwrap();
        assert_eq!(json["events"][0]["type"], "ai.request.completed");
    }

    // ── Client construction ────────────────────────────────────────

    #[test]
    fn client_creates_with_all_namespaces() {
        let client = HermonClient::new(
            HermonClientConfig::default(),
            Arc::new(MemoryTokenStore::new()),
        );
        // Just verify the client can be constructed and namespaces exist
        let _ = &client.auth;
        let _ = &client.ai;
        let _ = &client.agents;
        let _ = &client.conversations;
        let _ = &client.drive;
        let _ = &client.sessions;
        let _ = &client.orgs;
        let _ = &client.telemetry;
    }

    #[test]
    fn client_with_custom_config() {
        let config = HermonClientConfig {
            base_url: "https://api.hermon.ai".into(),
            user_agent: "wish-terminal/0.1.0".into(),
            retry: RetryConfig {
                max_attempts: 5,
                base_ms: 100,
                cap_ms: 5000,
            },
        };
        let client = HermonClient::new(config, Arc::new(MemoryTokenStore::new()));
        assert_eq!(client.http().config().base_url, "https://api.hermon.ai");
    }

    #[test]
    fn org_deserialize() {
        let json = r#"{
            "id": "o1",
            "name": "Hermon AI",
            "slug": "hermon-ai",
            "plan": "team",
            "createdAt": "2026-01-01T00:00:00Z"
        }"#;
        let o: types::session::Org = serde_json::from_str(json).unwrap();
        assert_eq!(o.name, "Hermon AI");
        assert_eq!(o.plan, "team");
    }

    #[test]
    fn model_filters_serializes_only_present() {
        let f = types::ai::ModelFilters {
            provider_id: Some("anthropic".into()),
            capability: None,
        };
        let json = serde_json::to_value(&f).unwrap();
        assert!(json.get("providerId").is_some());
        assert!(json.get("capability").is_none());
    }

    // ── Agent types ───────────────────────────────────────────────

    #[test]
    fn agent_deserialize() {
        let json = r#"{
            "id": "agent-1",
            "name": "Code Reviewer",
            "slug": "code-reviewer",
            "description": "Reviews pull requests",
            "model": {
                "providerId": "anthropic",
                "modelId": "claude-sonnet-4-6"
            },
            "tools": [{"toolId": "file_read", "requiresApproval": false}],
            "agentType": "sdlc",
            "capabilities": ["code_review", "suggestions"],
            "ownerId": "u1",
            "visibility": "org",
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z"
        }"#;
        let a: types::agent::Agent = serde_json::from_str(json).unwrap();
        assert_eq!(a.name, "Code Reviewer");
        assert_eq!(a.agent_type, types::agent::AgentType::Sdlc);
        assert_eq!(a.visibility, types::agent::AgentVisibility::Org);
        assert_eq!(a.tools.len(), 1);
    }

    #[test]
    fn create_agent_request_serializes() {
        let req = types::agent::CreateAgentRequest {
            name: "Planner".into(),
            slug: "planner".into(),
            description: Some("Plans projects".into()),
            model: types::agent::AgentModelConfig {
                provider_id: "anthropic".into(),
                model_id: "claude-sonnet-4-6".into(),
                fallback_model_id: None,
                temperature: Some(0.7),
                max_output_tokens: None,
            },
            tools: None,
            system_prompt: Some("You are a project planner.".into()),
            instructions: None,
            agent_type: types::agent::AgentType::Sdlc,
            capabilities: None,
            max_turns: Some(10),
            parameters: None,
            metadata: None,
            visibility: Some(types::agent::AgentVisibility::Private),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["name"], "Planner");
        assert_eq!(json["model"]["providerId"], "anthropic");
        assert_eq!(json["agentType"], "sdlc");
    }

    #[test]
    fn agent_stream_event_invocation_started() {
        let json = r#"{"kind":"invocation.started","invocationId":"inv-1","conversationId":"conv-1"}"#;
        let e: types::agent::AgentStreamEvent = serde_json::from_str(json).unwrap();
        matches!(e, types::agent::AgentStreamEvent::InvocationStarted { .. });
    }

    #[test]
    fn agent_stream_event_content_delta() {
        let json = r#"{"kind":"content.delta","delta":"hello world"}"#;
        let e: types::agent::AgentStreamEvent = serde_json::from_str(json).unwrap();
        matches!(e, types::agent::AgentStreamEvent::ContentDelta { .. });
    }

    #[test]
    fn agent_stream_event_tool_started() {
        let json = r#"{"kind":"tool.started","toolId":"t1","toolName":"file_read","input":{"path":"/tmp/x"}}"#;
        let e: types::agent::AgentStreamEvent = serde_json::from_str(json).unwrap();
        matches!(e, types::agent::AgentStreamEvent::ToolStarted { .. });
    }

    #[test]
    fn agent_stream_event_tool_approval() {
        let json = r#"{"kind":"tool.approval_required","toolId":"t1","toolName":"shell_exec","input":{"cmd":"rm -rf /"},"description":"Delete all files"}"#;
        let e: types::agent::AgentStreamEvent = serde_json::from_str(json).unwrap();
        matches!(
            e,
            types::agent::AgentStreamEvent::ToolApprovalRequired { .. }
        );
    }

    #[test]
    fn agent_stream_event_delegation() {
        let json = r#"{"kind":"delegation.started","targetAgentId":"agent-2","targetAgentName":"Tester","reason":"running test suite"}"#;
        let e: types::agent::AgentStreamEvent = serde_json::from_str(json).unwrap();
        matches!(e, types::agent::AgentStreamEvent::DelegationStarted { .. });
    }

    #[test]
    fn invoke_agent_request_serializes() {
        let req = types::agent::InvokeAgentRequest {
            conversation_id: Some("conv-1".into()),
            message: "Review this PR".into(),
            context: Some(vec![types::agent::AgentContext {
                context_type: types::agent::AgentContextType::Diff,
                content: "+line added\n-line removed".into(),
                metadata: None,
            }]),
            parameters: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["conversationId"], "conv-1");
        assert_eq!(json["context"][0]["type"], "diff");
    }

    // ── Drive types ───────────────────────────────────────────────

    #[test]
    fn drive_object_deserialize() {
        let json = r#"{
            "id": "obj-1",
            "name": "My Workflow",
            "objectType": "workflow",
            "ownerId": "u1",
            "visibility": "private",
            "contentHash": "abc123",
            "sizeBytes": 1024,
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z"
        }"#;
        let o: types::drive::DriveObject = serde_json::from_str(json).unwrap();
        assert_eq!(o.name, "My Workflow");
        assert_eq!(o.object_type, types::drive::DriveObjectType::Workflow);
        assert_eq!(o.visibility, types::drive::DriveVisibility::Private);
    }

    #[test]
    fn create_drive_object_serializes() {
        let req = types::drive::CreateDriveObjectRequest {
            name: "test.nb".into(),
            object_type: types::drive::DriveObjectType::Notebook,
            parent_id: Some("folder-1".into()),
            description: None,
            tags: Some(vec!["ai".into(), "test".into()]),
            metadata: None,
            visibility: Some(types::drive::DriveVisibility::Org),
            content: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["objectType"], "notebook");
        assert_eq!(json["parentId"], "folder-1");
        assert_eq!(json["tags"], serde_json::json!(["ai", "test"]));
    }

    #[test]
    fn drive_sync_state_deserialize() {
        let json = r#"{
            "cursor": "cur-abc",
            "hasMore": true,
            "changes": [{
                "objectId": "obj-1",
                "changeType": "updated",
                "timestamp": "2026-01-01T00:00:00Z"
            }]
        }"#;
        let s: types::drive::DriveSyncState = serde_json::from_str(json).unwrap();
        assert_eq!(s.cursor, "cur-abc");
        assert!(s.has_more);
        assert_eq!(s.changes[0].change_type, types::drive::SyncChangeType::Updated);
    }

    // ── Conversation types ────────────────────────────────────────

    #[test]
    fn conversation_deserialize() {
        let json = r#"{
            "id": "conv-1",
            "agentId": "agent-1",
            "userId": "u1",
            "title": "Code review session",
            "status": "active",
            "messageCount": 5,
            "totalTokens": 10000,
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z"
        }"#;
        let c: types::conversation::Conversation = serde_json::from_str(json).unwrap();
        assert_eq!(c.title.unwrap(), "Code review session");
        assert_eq!(c.status, types::conversation::ConversationStatus::Active);
        assert_eq!(c.message_count, 5);
    }

    #[test]
    fn conversation_message_deserialize() {
        let json = r#"{
            "id": "msg-1",
            "conversationId": "conv-1",
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Here is my review:"},
                {"type": "code", "code": "fn main() {}", "language": "rust"}
            ],
            "createdAt": "2026-01-01T00:00:00Z"
        }"#;
        let m: types::conversation::ConversationMessage = serde_json::from_str(json).unwrap();
        assert_eq!(m.role, types::conversation::MessageRole::Assistant);
        assert_eq!(m.content.len(), 2);
        match &m.content[0] {
            types::conversation::MessageBlock::Text { text } => {
                assert_eq!(text, "Here is my review:");
            }
            _ => panic!("expected text block"),
        }
    }

    #[test]
    fn message_block_tool_use_deserialize() {
        let json = r#"{"type": "tool_use", "toolId": "t1", "toolName": "file_read", "input": {"path": "/tmp"}}"#;
        let b: types::conversation::MessageBlock = serde_json::from_str(json).unwrap();
        matches!(b, types::conversation::MessageBlock::ToolUse { .. });
    }

    #[test]
    fn conversation_summary_deserialize() {
        let json = r#"{
            "id": "conv-1",
            "agentId": "agent-1",
            "agentName": "Code Reviewer",
            "title": "PR #42 Review",
            "status": "completed",
            "messageCount": 12,
            "lastMessagePreview": "LGTM!",
            "lastMessageAt": "2026-01-01T01:00:00Z",
            "createdAt": "2026-01-01T00:00:00Z"
        }"#;
        let s: types::conversation::ConversationSummary = serde_json::from_str(json).unwrap();
        assert_eq!(s.agent_name, "Code Reviewer");
        assert_eq!(s.status, types::conversation::ConversationStatus::Completed);
    }
}
