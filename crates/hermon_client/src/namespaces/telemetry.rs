//! Telemetry namespace — batch ingest.

use std::sync::Arc;

use crate::error::HermonError;
use crate::transport::http::RequestOptions;
use crate::transport::HermonHttp;
use crate::types::ai::TelemetryBatch;

/// Telemetry namespace on the Hermon client.
pub struct TelemetryNamespace {
    pub(crate) http: Arc<HermonHttp>,
}

impl TelemetryNamespace {
    /// Ingest a batch of telemetry events.
    pub async fn ingest(&self, batch: TelemetryBatch) -> Result<(), HermonError> {
        let _: serde_json::Value = self
            .http
            .request(
                RequestOptions::post("/v1/telemetry/ingest")
                    .with_body(&batch)
                    .with_idempotent(true),
            )
            .await?;
        Ok(())
    }
}
