use serde::Serialize;
use uuid::Uuid;

/// Single-item response envelope.
#[derive(Debug, Serialize)]
pub struct OkResponse<T: Serialize> {
    pub data: T,
    pub request_id: String,
}

impl<T: Serialize> OkResponse<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            request_id: Uuid::new_v4().to_string(),
        }
    }
}

/// Paginated list response envelope.
#[derive(Debug, Serialize)]
pub struct PageResponse<T: Serialize> {
    pub data: Vec<T>,
    pub meta: PageMeta,
    pub request_id: String,
}

#[derive(Debug, Serialize)]
pub struct PageMeta {
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}

impl<T: Serialize> PageResponse<T> {
    pub fn new(data: Vec<T>, total: i64, page: u32, per_page: u32) -> Self {
        Self {
            data,
            meta: PageMeta { total, page, per_page },
            request_id: Uuid::new_v4().to_string(),
        }
    }
}

/// Query parameters for paginated endpoints.
#[derive(Debug, serde::Deserialize)]
pub struct PageParams {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

fn default_page() -> u32 { 1 }
fn default_per_page() -> u32 { 20 }

impl PageParams {
    pub fn offset(&self) -> i64 {
        ((self.page.saturating_sub(1)) as i64) * (self.per_page as i64)
    }
    pub fn limit(&self) -> i64 {
        self.per_page.clamp(1, 100) as i64
    }
}
