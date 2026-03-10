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

    pub fn source_segment_duration_seconds(&self) -> u32 {
        self.baseline_rendition_profile.segment_duration_seconds
    }

    pub fn source_eligible_rendition_names(
        &self,
        source_width: u32,
        source_height: u32,
    ) -> Vec<String> {
        self.adaptive_rendition_ladder
            .iter()
            .filter(|profile| rendition_fits_source(profile.width, profile.height, source_width, source_height))
            .map(|profile| profile.name.clone())
            .collect()
    }

    pub fn source_eligible_additional_renditions(
        &self,
        source_width: u32,
        source_height: u32,
    ) -> Vec<String> {
        let highest_rendition_name = self
            .highest_eligible_rendition_profile(source_width, source_height)
            .map(|profile| profile.name);

        self.source_eligible_rendition_names(source_width, source_height)
            .into_iter()
            .filter(|name| name != &self.baseline_rendition_profile.name)
            .filter(|name| highest_rendition_name.as_ref() != Some(name))
            .collect()
    }

    pub fn highest_eligible_rendition_profile(
        &self,
        source_width: u32,
        source_height: u32,
    ) -> Option<AdaptiveRenditionProfile> {
        self.adaptive_rendition_ladder
            .iter()
            .rev()
            .find(|profile| rendition_fits_source(profile.width, profile.height, source_width, source_height))
            .cloned()
    }

    pub fn rendition_ladder_position(&self, rendition: &str) -> usize {
        self.adaptive_rendition_ladder
            .iter()
            .position(|profile| profile.name == rendition)
            .unwrap_or(usize::MAX)
    }
}

fn rendition_fits_source(
    rendition_width: u32,
    rendition_height: u32,
    source_width: u32,
    source_height: u32,
) -> bool {
    let source_long_edge = source_width.max(source_height);
    let source_short_edge = source_width.min(source_height);
    let rendition_long_edge = rendition_width.max(rendition_height);
    let rendition_short_edge = rendition_width.min(rendition_height);

    rendition_long_edge <= source_long_edge && rendition_short_edge <= source_short_edge
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselineRenditionProfile {
    pub name: String,
    pub segment_duration_seconds: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdaptiveRenditionProfile {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
}

#[cfg(test)]
mod tests {
    use super::{AdaptiveRenditionProfile, BaselineRenditionProfile, VideoPolicy};

    #[test]
    fn portrait_sources_keep_portrait_eligible_renditions() {
        let policy = VideoPolicy {
            baseline_rendition_profile: BaselineRenditionProfile {
                name: "360p".to_owned(),
                segment_duration_seconds: 4,
            },
            adaptive_rendition_ladder: vec![
                AdaptiveRenditionProfile {
                    name: "360p".to_owned(),
                    width: 640,
                    height: 360,
                    video_bitrate_kbps: 800,
                    audio_bitrate_kbps: 128,
                },
                AdaptiveRenditionProfile {
                    name: "1080p".to_owned(),
                    width: 1920,
                    height: 1080,
                    video_bitrate_kbps: 5000,
                    audio_bitrate_kbps: 192,
                },
                AdaptiveRenditionProfile {
                    name: "2160p".to_owned(),
                    width: 3840,
                    height: 2160,
                    video_bitrate_kbps: 16000,
                    audio_bitrate_kbps: 256,
                },
            ],
        };

        let renditions = policy.source_eligible_additional_renditions(2160, 3840);

        assert_eq!(renditions, vec!["1080p"]);
    }
}
