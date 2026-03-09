#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasterManifestVariant {
    pub rendition: String,
    pub playlist_path: String,
    pub width: u32,
    pub height: u32,
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
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

#[cfg(test)]
mod tests {
    use super::{render_master_manifest, MasterManifestVariant};

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
}
