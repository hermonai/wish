//! Drive namespace — object storage CRUD, sync, content access.

use std::sync::Arc;

use crate::error::HermonError;
use crate::transport::http::RequestOptions;
use crate::transport::HermonHttp;
use crate::types::common::{Page, PageParams};
use crate::types::drive::*;

/// Drive namespace on the Hermon client.
pub struct DriveNamespace {
    pub(crate) http: Arc<HermonHttp>,
}

impl DriveNamespace {
    /// Create a new drive object.
    pub async fn create(
        &self,
        input: CreateDriveObjectRequest,
    ) -> Result<DriveObject, HermonError> {
        let opts = RequestOptions::post("/v1/drive/objects").with_body(&input);
        self.http.request(opts).await
    }

    /// Get a drive object by ID.
    pub async fn get(&self, object_id: &str) -> Result<Option<DriveObject>, HermonError> {
        let path = format!("/v1/drive/objects/{object_id}");
        match self.http.request(RequestOptions::get(path)).await {
            Ok(o) => Ok(Some(o)),
            Err(HermonError::Server { status: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// List drive objects with optional filters and pagination.
    pub async fn list(
        &self,
        filters: Option<DriveListFilters>,
        page: Option<PageParams>,
    ) -> Result<Page<DriveObject>, HermonError> {
        let mut params = Vec::new();

        if let Some(f) = filters {
            if let Some(pid) = f.parent_id {
                params.push(("parentId".to_string(), pid));
            }
            if let Some(t) = f.object_type {
                params.push((
                    "objectType".to_string(),
                    serde_json::to_string(&t).unwrap().trim_matches('"').to_string(),
                ));
            }
            if let Some(v) = f.visibility {
                params.push((
                    "visibility".to_string(),
                    serde_json::to_string(&v).unwrap().trim_matches('"').to_string(),
                ));
            }
            if let Some(s) = f.search {
                params.push(("search".to_string(), s));
            }
            if let Some(tags) = f.tags {
                params.push(("tags".to_string(), tags.join(",")));
            }
            if let Some(del) = f.include_deleted {
                params.push(("includeDeleted".to_string(), del.to_string()));
            }
        }

        if let Some(p) = page {
            if let Some(o) = p.offset {
                params.push(("offset".to_string(), o.to_string()));
            }
            if let Some(l) = p.limit {
                params.push(("limit".to_string(), l.to_string()));
            }
        }

        let mut opts = RequestOptions::get("/v1/drive/objects");
        if !params.is_empty() {
            opts = opts.with_query(params);
        }

        self.http.request(opts).await
    }

    /// Update a drive object's metadata.
    pub async fn update(
        &self,
        object_id: &str,
        input: UpdateDriveObjectRequest,
    ) -> Result<DriveObject, HermonError> {
        let path = format!("/v1/drive/objects/{object_id}");
        let opts = RequestOptions::put(path).with_body(&input);
        self.http.request(opts).await
    }

    /// Soft-delete a drive object.
    pub async fn delete(&self, object_id: &str) -> Result<(), HermonError> {
        let path = format!("/v1/drive/objects/{object_id}");
        self.http.raw_request(&RequestOptions::delete(path)).await?;
        Ok(())
    }

    /// Get the content of a drive object.
    pub async fn get_content(
        &self,
        object_id: &str,
    ) -> Result<DriveObjectContent, HermonError> {
        let path = format!("/v1/drive/objects/{object_id}/content");
        self.http.request(RequestOptions::get(path)).await
    }

    /// Update the content of a drive object.
    pub async fn put_content(
        &self,
        object_id: &str,
        content: &serde_json::Value,
    ) -> Result<DriveObjectContent, HermonError> {
        let path = format!("/v1/drive/objects/{object_id}/content");
        let opts = RequestOptions::put(path).with_body(content);
        self.http.request(opts).await
    }

    /// Get incremental sync changes since a cursor.
    ///
    /// Pass `None` for initial sync to get all objects.
    pub async fn sync(&self, cursor: Option<&str>) -> Result<DriveSyncState, HermonError> {
        let mut params = Vec::new();
        if let Some(c) = cursor {
            params.push(("cursor".to_string(), c.to_string()));
        }

        let mut opts = RequestOptions::get("/v1/drive/sync");
        if !params.is_empty() {
            opts = opts.with_query(params);
        }

        self.http.request(opts).await
    }

    /// Move an object to a different parent folder.
    pub async fn move_object(
        &self,
        object_id: &str,
        new_parent_id: Option<&str>,
    ) -> Result<DriveObject, HermonError> {
        let path = format!("/v1/drive/objects/{object_id}/move");
        let body = serde_json::json!({ "parentId": new_parent_id });
        let opts = RequestOptions::post(path).with_body(&body);
        self.http.request(opts).await
    }

    /// List children of a folder.
    pub async fn list_children(
        &self,
        folder_id: &str,
        page: Option<PageParams>,
    ) -> Result<Page<DriveObject>, HermonError> {
        let filters = DriveListFilters {
            parent_id: Some(folder_id.to_string()),
            ..Default::default()
        };
        self.list(Some(filters), page).await
    }

    /// Restore a soft-deleted object.
    pub async fn restore(&self, object_id: &str) -> Result<DriveObject, HermonError> {
        let path = format!("/v1/drive/objects/{object_id}/restore");
        let opts = RequestOptions::post(path);
        self.http.request(opts).await
    }

    /// Duplicate a drive object.
    pub async fn duplicate(
        &self,
        object_id: &str,
        new_name: Option<&str>,
    ) -> Result<DriveObject, HermonError> {
        let path = format!("/v1/drive/objects/{object_id}/duplicate");
        let body = serde_json::json!({ "name": new_name });
        let opts = RequestOptions::post(path).with_body(&body);
        self.http.request(opts).await
    }
}
