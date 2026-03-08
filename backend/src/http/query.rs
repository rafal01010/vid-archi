use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListVideosQuery {
    pub page: Option<usize>,
    pub limit: Option<usize>,
    pub page_size: Option<usize>,
}
