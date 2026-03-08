use crate::domain::video_policy::RenditionProfile;

pub fn render_master_manifest(
    profile: &RenditionProfile,
    scaled_width: u32,
    scaled_height: u32,
) -> String {
    let bandwidth = (profile.video_bitrate_kbps + profile.audio_bitrate_kbps) * 1000;

    format!(
        "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-INDEPENDENT-SEGMENTS\n#EXT-X-STREAM-INF:BANDWIDTH={bandwidth},AVERAGE-BANDWIDTH={bandwidth},RESOLUTION={scaled_width}x{scaled_height},CODECS=\"avc1.42e01e,mp4a.40.2\"\n{}/{}\n",
        profile.name, profile.variant_playlist_file_name
    )
}
