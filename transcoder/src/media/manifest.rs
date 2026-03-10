#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasterManifestVariant {
    pub rendition: String,
    pub playlist_path: String,
    pub width: u32,
    pub height: u32,
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariantPlaylistSegment {
    pub duration_seconds: f64,
    pub segment_path: String,
}

pub fn render_master_manifest(variants: &[MasterManifestVariant]) -> String {
    let mut manifest = String::from("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-INDEPENDENT-SEGMENTS\n");

    for variant in variants {
        let bandwidth = (variant.video_bitrate_kbps + variant.audio_bitrate_kbps) * 1000;
        manifest.push_str(&format!(
            "#EXT-X-STREAM-INF:BANDWIDTH={bandwidth},AVERAGE-BANDWIDTH={bandwidth},RESOLUTION={}x{},CODECS=\"avc1.42e01e,mp4a.40.2\"\n{}\n",
            variant.width, variant.height, variant.playlist_path
        ));
    }

    manifest
}

pub fn render_variant_playlist(segments: &[VariantPlaylistSegment]) -> String {
    let target_duration = segments
        .iter()
        .map(|segment| segment.duration_seconds.ceil() as u32)
        .max()
        .unwrap_or(1);
    let mut manifest = format!(
        "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:{target_duration}\n#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n"
    );

    for (index, segment) in segments.iter().enumerate() {
        // Each output file is transcoded from an independently reset source segment.
        // Explicit discontinuities let HLS players safely join those standalone timelines.
        if index > 0 {
            manifest.push_str("#EXT-X-DISCONTINUITY\n");
        }

        manifest.push_str(&format!(
            "#EXTINF:{:.3},\n{}\n",
            segment.duration_seconds, segment.segment_path
        ));
    }

    manifest.push_str("#EXT-X-ENDLIST\n");
    manifest
}

#[cfg(test)]
mod tests {
    use super::{
        render_master_manifest, render_variant_playlist, MasterManifestVariant,
        VariantPlaylistSegment,
    };

    #[test]
    fn render_master_manifest_lists_all_ready_variants() {
        let manifest = render_master_manifest(&[
            MasterManifestVariant {
                rendition: "360p".to_owned(),
                playlist_path: "360p/360p.m3u8".to_owned(),
                width: 640,
                height: 360,
                video_bitrate_kbps: 800,
                audio_bitrate_kbps: 128,
            },
            MasterManifestVariant {
                rendition: "720p".to_owned(),
                playlist_path: "720p/720p.m3u8".to_owned(),
                width: 1280,
                height: 720,
                video_bitrate_kbps: 2800,
                audio_bitrate_kbps: 128,
            },
        ]);

        assert!(manifest.contains("360p/360p.m3u8"));
        assert!(manifest.contains("720p/720p.m3u8"));
    }

    #[test]
    fn render_variant_playlist_lists_segments_in_order() {
        let manifest = render_variant_playlist(&[
            VariantPlaylistSegment {
                duration_seconds: 4.0,
                segment_path: "segments/segment_00000.ts".to_owned(),
            },
            VariantPlaylistSegment {
                duration_seconds: 3.5,
                segment_path: "segments/segment_00001.ts".to_owned(),
            },
        ]);

        assert!(manifest.contains("#EXT-X-TARGETDURATION:4"));
        assert!(manifest.contains("segments/segment_00000.ts"));
        assert!(manifest.contains("segments/segment_00001.ts"));
        assert_eq!(manifest.matches("#EXT-X-DISCONTINUITY").count(), 1);
        assert!(manifest.contains("#EXT-X-ENDLIST"));
    }
}
