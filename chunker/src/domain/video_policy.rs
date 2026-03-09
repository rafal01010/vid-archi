use std::path::Path;

use serde::Deserialize;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoPolicy {
    pub baseline_rendition_profile: BaselineRenditionProfile,
    pub adaptive_rendition_ladder: Vec<AdaptiveRenditionProfile>,
}

impl VideoPolicy {
    pub async fn load(path: &Path) -> AppResult<Self> {
        let contents = tokio::fs::read_to_string(path).await.map_err(|error| {
            AppError::internal_with_context(
                "failed to read the video policy file",
                format!("path={} error={error}", path.display()),
            )
        })?;

        serde_json::from_str(&contents).map_err(|error| {
            AppError::internal_with_context(
                "failed to parse the video policy file",
                format!("path={} error={error}", path.display()),
            )
        })
    }

    pub fn baseline_rendition_name(&self) -> String {
        self.baseline_rendition_profile.name.clone()
    }

    pub fn source_eligible_additional_renditions(
        &self,
        source_width: u32,
        source_height: u32,
    ) -> Vec<String> {
        self.adaptive_rendition_ladder
            .iter()
            .filter(|profile| {
                profile.name != self.baseline_rendition_profile.name
                    && profile.width <= source_width
                    && profile.height <= source_height
            })
            .map(|profile| profile.name.clone())
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselineRenditionProfile {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdaptiveRenditionProfile {
    pub name: String,
    pub width: u32,
    pub height: u32,
}
