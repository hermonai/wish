use std::{collections::HashMap, sync::Arc};

use crate::{ai::agent::redaction, terminal::model::session::SessionType};
use futures_util::StreamExt;
use wish_core::features::FeatureFlag;
use wish_multi_agent_api as api;

use crate::server::server_api::ServerApi;

use super::{convert_to::convert_input, ConvertToAPITypeError, RequestParams, ResponseStream};

pub async fn generate_multi_agent_output(
    server_api: Arc<ServerApi>,
    mut params: RequestParams,
    cancellation_rx: futures::channel::oneshot::Receiver<()>,
) -> Result<ResponseStream, ConvertToAPITypeError> {
    // Route to local Ollama when the selected model is an Ollama model.
    // This skips the hermon server auth flow entirely.
    if params.model.as_str().starts_with("ollama:") {
        return generate_local_ollama_output(params, cancellation_rx).await;
    }

    let supported_tools = params
        .supported_tools_override
        .take()
        .unwrap_or_else(|| get_supported_tools(&params));
    let supported_cli_agent_tools = get_supported_cli_agent_tools(&params);
    let mut logging_metadata = HashMap::new();
    if let Some(metadata) = params.metadata {
        logging_metadata.insert(
            "is_autodetected_user_query".to_owned(),
            prost_types::Value {
                kind: Some(prost_types::value::Kind::BoolValue(
                    metadata.is_autodetected_user_query,
                )),
            },
        );
        logging_metadata.insert(
            "entrypoint".to_owned(),
            prost_types::Value {
                kind: Some(prost_types::value::Kind::StringValue(
                    metadata.entrypoint.entrypoint(),
                )),
            },
        );
        logging_metadata.insert(
            "is_auto_resume_after_error".to_owned(),
            prost_types::Value {
                kind: Some(prost_types::value::Kind::BoolValue(
                    metadata.is_auto_resume_after_error,
                )),
            },
        );
    }

    if params.should_redact_secrets {
        redaction::redact_inputs(&mut params.input);
    }

    let mut api_keys = params.api_keys;
    if let Some(api_keys) = &mut api_keys {
        api_keys.allow_use_of_warp_credits = params.allow_use_of_warp_credits_with_byok;
    }

    let request = api::Request {
        task_context: Some(api::request::TaskContext {
            tasks: params.tasks,
        }),
        input: Some(convert_input(params.input)?),
        settings: Some(api::request::Settings {
            model_config: Some(api::request::settings::ModelConfig {
                base: params.model.into(),
                cli_agent: params.cli_agent_model.into(),
                computer_use_agent: params.computer_use_model.into(),
                base_model_context_window_limit: if FeatureFlag::ConfigurableContextWindow
                    .is_enabled()
                {
                    params.context_window_limit.unwrap_or(0)
                } else {
                    0
                },
                ..Default::default()
            }),
            rules_enabled: params.is_memory_enabled,
            warp_drive_context_enabled: params.warp_drive_context_enabled,
            web_context_retrieval_enabled: true,
            supports_parallel_tool_calls: true,
            use_anthropic_text_editor_tools: false,
            planning_enabled: params.planning_enabled,
            supports_create_files: true,
            supported_tools: supported_tools.into_iter().map(Into::into).collect(),
            supports_long_running_commands: true,
            should_preserve_file_content_in_history: true,
            supports_todos_ui: true,
            supports_linked_code_blocks: FeatureFlag::LinkedCodeBlocks.is_enabled(),
            supports_started_child_task_message: true,
            supports_suggest_prompt: true,
            supports_read_image_files: FeatureFlag::ReadImageFiles.is_enabled(),
            supports_reasoning_message: true,
            api_keys,
            autonomy_level: params.autonomy_level.into(),
            isolation_level: params.isolation_level.into(),
            web_search_enabled: params.web_search_enabled,
            supported_cli_agent_tools: supported_cli_agent_tools
                .into_iter()
                .map(Into::into)
                .collect(),
            supports_v4a_file_diffs: FeatureFlag::V4AFileDiffs.is_enabled(),
            supports_summarization_via_message_replacement:
                FeatureFlag::SummarizationViaMessageReplacement.is_enabled(),
            supports_bundled_skills: FeatureFlag::BundledSkills.is_enabled(),
            supports_research_agent: params.research_agent_enabled,
            supports_orchestration_v2: FeatureFlag::OrchestrationV2.is_enabled(),
            custom_model_providers: None,
        }),
        metadata: Some(api::request::Metadata {
            logging: logging_metadata,
            conversation_id: params
                .conversation_token
                .as_ref()
                .map(|token| token.as_str().to_string())
                .unwrap_or_default(),
            ambient_agent_task_id: params
                .ambient_agent_task_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
            forked_from_conversation_id: if params.conversation_token.is_none() {
                // We only include this param on our initial request to the server
                // (when the forked conversation has not been assigned a new id yet).
                params
                    .forked_from_conversation_token
                    .map(|token| token.as_str().to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            },
            parent_agent_id: params.parent_agent_id.unwrap_or_default(),
            agent_name: params.agent_name.unwrap_or_default(),
        }),
        existing_suggestions: params
            .existing_suggestions
            .map(|suggestions| suggestions.into()),
        mcp_context: params.mcp_context.map(Into::into),
    };

    let response_stream = server_api.generate_multi_agent_output(&request).await;
    match response_stream {
        Ok(stream) => {
            let output_stream = stream.take_until(cancellation_rx);
            Ok(Box::pin(output_stream))
        }
        Err(e) => {
            let (tx, rx) = async_channel::unbounded();
            let _ = tx.send(Err(e)).await;
            Ok(Box::pin(rx))
        }
    }
}

fn get_supported_tools(params: &RequestParams) -> Vec<api::ToolType> {
    let mut supported_tools = vec![
        api::ToolType::Grep,
        api::ToolType::FileGlob,
        api::ToolType::FileGlobV2,
        api::ToolType::ReadMcpResource,
        api::ToolType::CallMcpTool,
        api::ToolType::InitProject,
        api::ToolType::OpenCodeReview,
        api::ToolType::RunShellCommand,
        api::ToolType::SuggestNewConversation,
        api::ToolType::Subagent,
        api::ToolType::WriteToLongRunningShellCommand,
        api::ToolType::ReadShellCommandOutput,
        api::ToolType::ReadDocuments,
        api::ToolType::CreateDocuments,
        api::ToolType::EditDocuments,
        api::ToolType::SuggestPrompt,
    ];

    if FeatureFlag::ConversationsAsContext.is_enabled() {
        supported_tools.push(api::ToolType::FetchConversation);
    }

    match params.session_context.session_type() {
        None | Some(SessionType::Local) => {
            supported_tools.extend(&[
                api::ToolType::ReadFiles,
                api::ToolType::ApplyFileDiffs,
                api::ToolType::SearchCodebase,
            ]);

            if FeatureFlag::ArtifactCommand.is_enabled() {
                supported_tools.push(api::ToolType::UploadFileArtifact);
            }
        }
        Some(SessionType::WishifiedRemote { host_id: Some(_) }) => {
            // Remote session with a known host — enable tools that route
            // through RemoteServerClient. The host_id is only populated
            // after a successful connection handshake, so its presence is a
            // sufficient proxy for client availability.
            supported_tools.extend(&[api::ToolType::ReadFiles, api::ToolType::ApplyFileDiffs]);
            if FeatureFlag::RemoteCodebaseIndexing.is_enabled()
                && params.remote_codebase_search_available
            {
                supported_tools.push(api::ToolType::SearchCodebase);
            }
        }
        Some(SessionType::WishifiedRemote { host_id: None }) => {
            // Feature flag off or not yet connected — no remote tools.
        }
    }

    if FeatureFlag::AgentModeComputerUse.is_enabled() && params.computer_use_enabled {
        supported_tools.extend(&[api::ToolType::UseComputer]);
        supported_tools.extend(&[api::ToolType::RequestComputerUse])
    }

    if FeatureFlag::PRCommentsSlashCommand.is_enabled() {
        supported_tools.push(api::ToolType::InsertReviewComments);
    }

    if FeatureFlag::ListSkills.is_enabled() {
        supported_tools.push(api::ToolType::ReadSkill);
    }

    if params.orchestration_enabled {
        // Always advertise the legacy start-agent tool so the server
        // can fall back to it when its own orchestrate flag is off.
        // When RunAgents is also enabled, advertise it alongside.
        supported_tools.push(if FeatureFlag::OrchestrationV2.is_enabled() {
            api::ToolType::StartAgentV2
        } else {
            api::ToolType::StartAgent
        });
        if FeatureFlag::RunAgentsTool.is_enabled() && FeatureFlag::OrchestrationV2.is_enabled() {
            supported_tools.push(api::ToolType::RunAgents);
        }
        supported_tools.push(api::ToolType::SendMessageToAgent);
    }

    if FeatureFlag::AskUserQuestion.is_enabled() && params.ask_user_question_enabled {
        supported_tools.push(api::ToolType::AskUserQuestion);
    }

    supported_tools
}

fn get_supported_cli_agent_tools(params: &RequestParams) -> Vec<api::ToolType> {
    let mut supported_cli_agent_tools = vec![
        api::ToolType::WriteToLongRunningShellCommand,
        api::ToolType::ReadShellCommandOutput,
        api::ToolType::Grep,
        api::ToolType::FileGlob,
        api::ToolType::FileGlobV2,
    ];

    if FeatureFlag::TransferControlTool.is_enabled() {
        supported_cli_agent_tools.push(api::ToolType::TransferShellCommandControlToUser);
    }

    match params.session_context.session_type() {
        None | Some(SessionType::Local) => {
            supported_cli_agent_tools
                .extend(&[api::ToolType::ReadFiles, api::ToolType::SearchCodebase]);
        }
        Some(SessionType::WishifiedRemote { host_id: Some(_) }) => {
            supported_cli_agent_tools.push(api::ToolType::ReadFiles);
            if FeatureFlag::RemoteCodebaseIndexing.is_enabled()
                && params.remote_codebase_search_available
            {
                supported_cli_agent_tools.push(api::ToolType::SearchCodebase);
            }
        }
        Some(SessionType::WishifiedRemote { host_id: None }) => {}
    }

    supported_cli_agent_tools
}

// ── Local Ollama agent mode ──────────────────────────────────────────

/// Drive an agent-mode request against a local Ollama endpoint instead
/// of the remote Hermon server. Builds a minimal OpenAI-compatible
/// chat-completion request, sends it to Ollama, and wraps the response
/// in the `wish_multi_agent_api::ResponseEvent` stream format that the
/// rest of the agent-mode UI expects.
async fn generate_local_ollama_output(
    params: RequestParams,
    cancellation_rx: futures::channel::oneshot::Receiver<()>,
) -> Result<ResponseStream, ConvertToAPITypeError> {
    use crate::ai::llms::ollama_base_url;
    use crate::ai::local_llm::{ChatCompletionRequest, ChatCompletionResponse, ChatMessage};
    use crate::ai::wish_conversation::local_llm_adapter::chat_completions_url;
    use crate::server::server_api::AIApiError;

    let model_name = params
        .model
        .as_str()
        .strip_prefix("ollama:")
        .unwrap_or(params.model.as_str())
        .to_string();

    // Extract user message from the input.
    let user_message = params
        .input
        .iter()
        .find_map(|input| input.user_query())
        .unwrap_or_default();

    let base_url = ollama_base_url();
    // Ollama's OpenAI-compatible endpoint lives under /v1.
    let url = chat_completions_url(&format!("{base_url}/v1"));

    let request = ChatCompletionRequest {
        model: model_name,
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: user_message,
        }],
        temperature: None,
        max_tokens: None,
        stream: false,
    };

    // Run the HTTP request (async).
    let http_result =
        async {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .map_err(|e| format!("HTTP client init failed: {e}"))?;

            let resp =
                client.post(&url).json(&request).send().await.map_err(|e| {
                    format!("Ollama request failed (is Ollama running at {url}?): {e}")
                })?;

            let status = resp.status();
            let text = resp
                .text()
                .await
                .map_err(|e| format!("Ollama response read failed: {e}"))?;

            if !status.is_success() {
                return Err(format!(
                    "Ollama returned HTTP {}: {}",
                    status.as_u16(),
                    text
                ));
            }

            let parsed: ChatCompletionResponse = serde_json::from_str(&text)
                .map_err(|e| format!("Ollama response parse error: {e}"))?;

            let response_text = parsed
                .choices
                .into_iter()
                .next()
                .map(|c| c.message.content)
                .unwrap_or_else(|| "No response from model.".to_string());

            Ok::<String, String>(response_text)
        }
        .await;

    // Build ResponseEvent stream.
    let task_id = uuid::Uuid::new_v4().to_string();
    let message_id = uuid::Uuid::new_v4().to_string();
    let request_id = uuid::Uuid::new_v4().to_string();

    match http_result {
        Ok(response_text) => {
            let events: Vec<Result<api::ResponseEvent, Arc<AIApiError>>> = vec![
                // 1. StreamInit
                Ok(api::ResponseEvent {
                    r#type: Some(api::response_event::Type::Init(
                        api::response_event::StreamInit {
                            conversation_id: String::new(),
                            request_id: request_id.clone(),
                            run_id: String::new(),
                        },
                    )),
                }),
                // 2. ClientActions: CreateTask + AddMessagesToTask with AgentOutput
                Ok(api::ResponseEvent {
                    r#type: Some(api::response_event::Type::ClientActions(
                        api::response_event::ClientActions {
                            actions: vec![
                                api::ClientAction {
                                    action: Some(api::client_action::Action::CreateTask(
                                        api::client_action::CreateTask {
                                            task: Some(api::Task {
                                                id: task_id.clone(),
                                                description: String::new(),
                                                dependencies: None,
                                                messages: vec![],
                                                summary: String::new(),
                                                server_data: String::new(),
                                            }),
                                        },
                                    )),
                                },
                                api::ClientAction {
                                    action: Some(api::client_action::Action::AddMessagesToTask(
                                        api::client_action::AddMessagesToTask {
                                            task_id: task_id.clone(),
                                            messages: vec![api::Message {
                                                id: message_id,
                                                task_id: task_id.clone(),
                                                request_id: request_id.clone(),
                                                timestamp: None,
                                                server_message_data: String::new(),
                                                citations: vec![],
                                                message: Some(api::message::Message::AgentOutput(
                                                    api::message::AgentOutput {
                                                        text: response_text,
                                                    },
                                                )),
                                            }],
                                        },
                                    )),
                                },
                            ],
                        },
                    )),
                }),
                // 3. StreamFinished with Done
                Ok(api::ResponseEvent {
                    r#type: Some(api::response_event::Type::Finished(
                        api::response_event::StreamFinished {
                            token_usage: vec![],
                            should_refresh_model_config: false,
                            request_cost: None,
                            conversation_usage_metadata: None,
                            reason: Some(api::response_event::stream_finished::Reason::Done(
                                api::response_event::stream_finished::Done {},
                            )),
                        },
                    )),
                }),
            ];

            let stream = futures_lite::stream::iter(events);
            let output_stream = stream.take_until(cancellation_rx);
            Ok(Box::pin(output_stream))
        }
        Err(error_message) => {
            let (tx, rx) = async_channel::unbounded();
            let _ = tx
                .send(Err(Arc::new(AIApiError::Other(anyhow::anyhow!(
                    "{error_message}"
                )))))
                .await;
            Ok(Box::pin(rx))
        }
    }
}

#[cfg(test)]
#[path = "impl_tests.rs"]
mod tests;
