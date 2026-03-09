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

    pub fn is_baseline_rendition(&self, rendition: &str) -> bool {
        self.baseline_rendition_profile.name == rendition
    }

    pub fn source_eligible_rendition_names(
        &self,
        source_width: u32,
        source_height: u32,
    ) -> Vec<String> {
        let mut renditions = vec![self.baseline_rendition_profile.name.clone()];

        renditions.extend(
            self.adaptive_rendition_ladder
                .iter()
                .filter(|profile| {
                    profile.name != self.baseline_rendition_profile.name
                        && rendition_fits_source(
                            profile.width,
                            profile.height,
                            source_width,
                            source_height,
                        )
                })
                .map(|profile| profile.name.clone()),
        );

        renditions
    }

    pub fn rendition_ladder_position(&self, rendition: &str) -> usize {
        self.adaptive_rendition_ladder
            .iter()
            .position(|profile| profile.name == rendition)
            .unwrap_or(usize::MAX)
    }

    pub fn find_rendition_profile(&self, rendition: &str) -> Option<RenditionProfile> {
        if self.baseline_rendition_profile.name == rendition {
            return Some(RenditionProfile {
                name: self.baseline_rendition_profile.name.clone(),
                segment_container: self.baseline_rendition_profile.segment_container.clone(),
                video_codec: self.baseline_rendition_profile.video_codec.clone(),
                width: self.baseline_rendition_profile.width,
                height: self.baseline_rendition_profile.height,
                max_frame_rate: self.baseline_rendition_profile.max_frame_rate,
                video_bitrate_kbps: self.baseline_rendition_profile.video_bitrate_kbps,
                audio_bitrate_kbps: self.baseline_rendition_profile.audio_bitrate_kbps,
                segment_duration_seconds: self.baseline_rendition_profile.segment_duration_seconds,
                master_playlist_file_name: Some(
                    self.baseline_rendition_profile
                        .master_playlist_file_name
                        .clone(),
                ),
                variant_playlist_file_name: self
                    .baseline_rendition_profile
                    .variant_playlist_file_name
                    .clone(),
            });
        }

        self.adaptive_rendition_ladder
            .iter()
            .find(|profile| profile.name == rendition)
            .map(|profile| RenditionProfile {
                name: profile.name.clone(),
                segment_container: self.baseline_rendition_profile.segment_container.clone(),
                video_codec: self.baseline_rendition_profile.video_codec.clone(),
                width: profile.width,
                height: profile.height,
                max_frame_rate: self.baseline_rendition_profile.max_frame_rate,
                video_bitrate_kbps: profile.video_bitrate_kbps,
                audio_bitrate_kbps: profile.audio_bitrate_kbps,
                segment_duration_seconds: self.baseline_rendition_profile.segment_duration_seconds,
                master_playlist_file_name: None,
                variant_playlist_file_name: profile.variant_playlist_file_name.clone(),
            })
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
    pub segment_container: String,
    pub video_codec: String,
    pub width: u32,
    pub height: u32,
    pub max_frame_rate: u32,
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
    pub segment_duration_seconds: u32,
    pub master_playlist_file_name: String,
    pub variant_playlist_file_name: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdaptiveRenditionProfile {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
    pub variant_playlist_file_name: String,
}

#[derive(Clone, Debug)]
pub struct RenditionProfile {
    pub name: String,
    pub segment_container: String,
    pub video_codec: String,
    pub width: u32,
    pub height: u32,
    pub max_frame_rate: u32,
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
    pub segment_duration_seconds: u32,
    pub master_playlist_file_name: Option<String>,
    pub variant_playlist_file_name: String,
}

#[cfg(test)]
mod tests {
    use super::{AdaptiveRenditionProfile, BaselineRenditionProfile, VideoPolicy};

    #[test]
    fn baseline_lookup_prefers_baseline_profile() {
        let policy = VideoPolicy {
            baseline_rendition_profile: BaselineRenditionProfile {
                name: "360p".to_owned(),
                segment_container: "mpegts".to_owned(),
                video_codec: "h264".to_owned(),
                width: 640,
                height: 360,
                max_frame_rate: 30,
                video_bitrate_kbps: 800,
                audio_bitrate_kbps: 128,
                segment_duration_seconds: 4,
                master_playlist_file_name: "master.m3u8".to_owned(),
                variant_playlist_file_name: "360p.m3u8".to_owned(),
            },
            adaptive_rendition_ladder: vec![AdaptiveRenditionProfile {
                name: "720p".to_owned(),
                width: 1280,
                height: 720,
                video_bitrate_kbps: 2800,
                audio_bitrate_kbps: 128,
                variant_playlist_file_name: "720p.m3u8".to_owned(),
            }],
        };

        let profile = policy
            .find_rendition_profile("360p")
            .expect("baseline profile");

        assert_eq!(profile.name, "360p");
        assert!(policy.is_baseline_rendition("360p"));
    }

    #[test]
    fn adaptive_lookup_reuses_baseline_defaults() {
        let policy = VideoPolicy {
            baseline_rendition_profile: BaselineRenditionProfile {
                name: "360p".to_owned(),
                segment_container: "mpegts".to_owned(),
                video_codec: "h264".to_owned(),
                width: 640,
                height: 360,
                max_frame_rate: 30,
                video_bitrate_kbps: 800,
                audio_bitrate_kbps: 128,
                segment_duration_seconds: 4,
                master_playlist_file_name: "master.m3u8".to_owned(),
                variant_playlist_file_name: "360p.m3u8".to_owned(),
            },
            adaptive_rendition_ladder: vec![AdaptiveRenditionProfile {
                name: "720p".to_owned(),
                width: 1280,
                height: 720,
                video_bitrate_kbps: 2800,
                audio_bitrate_kbps: 128,
                variant_playlist_file_name: "720p.m3u8".to_owned(),
            }],
        };

        let profile = policy
            .find_rendition_profile("720p")
            .expect("adaptive profile");

        assert_eq!(profile.segment_container, "mpegts");
        assert_eq!(profile.video_codec, "h264");
        assert!(profile.master_playlist_file_name.is_none());
    }

    #[test]
    fn source_eligible_renditions_keep_baseline_and_skip_upscale_targets() {
        let policy = VideoPolicy {
            baseline_rendition_profile: BaselineRenditionProfile {
                name: "360p".to_owned(),
                segment_container: "mpegts".to_owned(),
                video_codec: "h264".to_owned(),
                width: 640,
                height: 360,
                max_frame_rate: 30,
                video_bitrate_kbps: 800,
                audio_bitrate_kbps: 128,
                segment_duration_seconds: 4,
                master_playlist_file_name: "master.m3u8".to_owned(),
                variant_playlist_file_name: "360p.m3u8".to_owned(),
            },
            adaptive_rendition_ladder: vec![
                AdaptiveRenditionProfile {
                    name: "360p".to_owned(),
                    width: 640,
                    height: 360,
                    video_bitrate_kbps: 800,
                    audio_bitrate_kbps: 128,
                    variant_playlist_file_name: "360p.m3u8".to_owned(),
                },
                AdaptiveRenditionProfile {
                    name: "480p".to_owned(),
                    width: 854,
                    height: 480,
                    video_bitrate_kbps: 1400,
                    audio_bitrate_kbps: 128,
                    variant_playlist_file_name: "480p.m3u8".to_owned(),
                },
                AdaptiveRenditionProfile {
                    name: "720p".to_owned(),
                    width: 1280,
                    height: 720,
                    video_bitrate_kbps: 2800,
                    audio_bitrate_kbps: 128,
                    variant_playlist_file_name: "720p.m3u8".to_owned(),
                },
            ],
        };

        let renditions = policy.source_eligible_rendition_names(1280, 720);

        assert_eq!(renditions, vec!["360p", "480p", "720p"]);

        let renditions = policy.source_eligible_rendition_names(854, 480);

        assert_eq!(renditions, vec!["360p", "480p"]);
    }

    #[test]
    fn portrait_sources_keep_high_portrait_eligible_renditions() {
        let policy = VideoPolicy {
            baseline_rendition_profile: BaselineRenditionProfile {
                name: "360p".to_owned(),
                segment_container: "mpegts".to_owned(),
                video_codec: "h264".to_owned(),
                width: 640,
                height: 360,
                max_frame_rate: 30,
                video_bitrate_kbps: 800,
                audio_bitrate_kbps: 128,
                segment_duration_seconds: 4,
                master_playlist_file_name: "master.m3u8".to_owned(),
                variant_playlist_file_name: "360p.m3u8".to_owned(),
            },
            adaptive_rendition_ladder: vec![
                AdaptiveRenditionProfile {
                    name: "360p".to_owned(),
                    width: 640,
                    height: 360,
                    video_bitrate_kbps: 800,
                    audio_bitrate_kbps: 128,
                    variant_playlist_file_name: "360p.m3u8".to_owned(),
                },
                AdaptiveRenditionProfile {
                    name: "1080p".to_owned(),
                    width: 1920,
                    height: 1080,
                    video_bitrate_kbps: 4500,
                    audio_bitrate_kbps: 192,
                    variant_playlist_file_name: "1080p.m3u8".to_owned(),
                },
                AdaptiveRenditionProfile {
                    name: "2160p".to_owned(),
                    width: 3840,
                    height: 2160,
                    video_bitrate_kbps: 12000,
                    audio_bitrate_kbps: 192,
                    variant_playlist_file_name: "2160p.m3u8".to_owned(),
                },
            ],
        };

        let renditions = policy.source_eligible_rendition_names(2160, 3840);

        assert_eq!(renditions, vec!["360p", "1080p", "2160p"]);
    }
}
