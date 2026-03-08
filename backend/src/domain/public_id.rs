use std::path::Path;

use data_encoding::BASE32_NOPAD;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct PublicIdGenerator;

impl PublicIdGenerator {
    pub fn generate(
        &self,
        video_id: Uuid,
        title: Option<&str>,
        filename: &str,
    ) -> Result<String, String> {
        let source_text = title
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(filename);
        let slug = slugify(extract_slug_source(source_text));
        let token = BASE32_NOPAD.encode(video_id.as_bytes()).to_ascii_lowercase();

        if token.is_empty() {
            return Err("failed to derive a public id token".to_owned());
        }

        if slug.is_empty() {
            return Ok(token);
        }

        Ok(format!("{slug}-{token}"))
    }
}

fn extract_slug_source(source_text: &str) -> &str {
    Path::new(source_text)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(source_text)
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_was_separator = false;

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            previous_was_separator = false;
            continue;
        }

        if !previous_was_separator {
            slug.push('-');
            previous_was_separator = true;
        }
    }

    let trimmed_slug = slug.trim_matches('-');
    let truncated_slug = trimmed_slug
        .chars()
        .take(48)
        .collect::<String>()
        .trim_matches('-')
        .to_owned();

    truncated_slug
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::PublicIdGenerator;

    #[test]
    fn generates_deterministic_public_id_from_title() {
        let generator = PublicIdGenerator::default();
        let video_id =
            Uuid::parse_str("8ee7b885-c177-4438-94f7-f632d4d64af4").expect("valid uuid");

        let public_id = generator
            .generate(video_id, Some("My Demo Upload"), "ignored.mp4")
            .expect("public id");

        assert_eq!(public_id, "my-demo-upload-r3t3rbobo5cdrfhx6yznjvsk6q");
    }

    #[test]
    fn falls_back_to_filename_when_title_is_missing() {
        let generator = PublicIdGenerator::default();
        let video_id =
            Uuid::parse_str("8ee7b885-c177-4438-94f7-f632d4d64af4").expect("valid uuid");

        let public_id = generator
            .generate(video_id, None, "Summer Vacation 2026.mov")
            .expect("public id");

        assert!(public_id.starts_with("summer-vacation-2026-"));
    }

    #[test]
    fn collapses_non_alphanumeric_characters_into_single_separators() {
        let generator = PublicIdGenerator::default();
        let video_id =
            Uuid::parse_str("8ee7b885-c177-4438-94f7-f632d4d64af4").expect("valid uuid");

        let public_id = generator
            .generate(video_id, Some("  Big___Title!!! With   Spaces "), "demo.mp4")
            .expect("public id");

        assert!(public_id.starts_with("big-title-with-spaces-"));
        assert!(!public_id.contains("--"));
    }
}
