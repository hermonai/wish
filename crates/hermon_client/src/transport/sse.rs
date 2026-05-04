//! SSE stream reader for Hermon streaming endpoints.
//!
//! Uses reqwest's byte streaming (not EventSource) to support POST
//! body + custom headers. Parses SSE line protocol manually.

use futures::stream::Stream;
use futures::StreamExt;

use crate::error::HermonError;

/// Parse a raw byte stream as SSE events, yielding deserialized `T` for
/// each `data:` payload. The stream ends on an event with type "done" or
/// "end", or when the connection closes.
pub fn parse_sse_stream<T: serde::de::DeserializeOwned + Send + 'static>(
    response: reqwest::Response,
) -> impl Stream<Item = Result<T, HermonError>> {
    let byte_stream = response.bytes_stream();

    async_stream::stream! {
        let mut buffer = String::new();
        let mut event_type = String::new();
        let mut data_lines: Vec<String> = Vec::new();

        futures::pin_mut!(byte_stream);

        while let Some(chunk_result) = byte_stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    yield Err(HermonError::Stream {
                        message: format!("stream read error: {e}"),
                    });
                    return;
                }
            };

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete lines
            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    // Blank line = event boundary
                    if !data_lines.is_empty() {
                        // Check for terminal event types
                        if event_type == "done" || event_type == "end" {
                            return;
                        }

                        let data = data_lines.join("\n");
                        data_lines.clear();
                        event_type.clear();

                        if data.is_empty() {
                            continue;
                        }

                        match serde_json::from_str::<T>(&data) {
                            Ok(parsed) => yield Ok(parsed),
                            Err(e) => {
                                yield Err(HermonError::Stream {
                                    message: format!("failed to parse SSE data: {e}"),
                                });
                                return;
                            }
                        }
                    }
                    continue;
                }

                if let Some(value) = line.strip_prefix("event:") {
                    event_type = value.trim().to_string();
                } else if let Some(value) = line.strip_prefix("data:") {
                    data_lines.push(value.trim().to_string());
                } else if line.starts_with("id:") || line.starts_with("retry:") {
                    // Ignored for now
                } else if line.starts_with(':') {
                    // Comment, ignore
                }
            }
        }

        // Process remaining data if connection closed without blank line
        if !data_lines.is_empty() {
            let data = data_lines.join("\n");
            if !data.is_empty() {
                match serde_json::from_str::<T>(&data) {
                    Ok(parsed) => yield Ok(parsed),
                    Err(e) => {
                        yield Err(HermonError::Stream {
                            message: format!("failed to parse final SSE data: {e}"),
                        });
                    }
                }
            }
        }
    }
}
