#!/usr/bin/env python3
"""
Create a Markdown transcript from a YouTube URL.

Usage:
  python helper_scripts/youtube_transcript_to_md.py "https://www.youtube.com/watch?v=VIDEO_ID"
  python helper_scripts/youtube_transcript_to_md.py "https://youtu.be/VIDEO_ID" -o transcripts/my_video.md

Dependency:
  pip install youtube-transcript-api
"""

from __future__ import annotations

import argparse
import html
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import parse_qs, quote_plus, urlparse
from urllib.request import urlopen

try:
    from youtube_transcript_api import YouTubeTranscriptApi
except ImportError:  # pragma: no cover
    print(
        "Missing dependency: youtube-transcript-api\n"
        "Install with: pip install youtube-transcript-api",
        file=sys.stderr,
    )
    sys.exit(1)


VIDEO_ID_RE = re.compile(r"^[A-Za-z0-9_-]{11}$")
INVALID_FILENAME_CHARS_RE = re.compile(r'[<>:"/\\|?*\x00-\x1F]')


def extract_video_id(url_or_id: str) -> str:
    raw = url_or_id.strip()
    if VIDEO_ID_RE.fullmatch(raw):
        return raw

    parsed = urlparse(raw)
    host = parsed.netloc.lower().replace("www.", "")

    if host == "youtu.be":
        candidate = parsed.path.strip("/").split("/")[0]
        if VIDEO_ID_RE.fullmatch(candidate):
            return candidate

    if host in {"youtube.com", "m.youtube.com"}:
        if parsed.path == "/watch":
            candidate = parse_qs(parsed.query).get("v", [None])[0]
            if candidate and VIDEO_ID_RE.fullmatch(candidate):
                return candidate

        for prefix in ("/shorts/", "/live/", "/embed/"):
            if parsed.path.startswith(prefix):
                candidate = parsed.path[len(prefix) :].split("/")[0]
                if VIDEO_ID_RE.fullmatch(candidate):
                    return candidate

    raise ValueError(f"Could not extract a valid YouTube video ID from: {url_or_id}")


def normalize_text(text: str) -> str:
    text = html.unescape(text).replace("\n", " ")
    text = re.sub(r"\s+", " ", text).strip()
    return text


def fetch_transcript(video_id: str, languages: list[str]) -> list[dict]:
    # youtube-transcript-api >=1.0.0
    if hasattr(YouTubeTranscriptApi, "fetch"):
        api = YouTubeTranscriptApi()
        try:
            fetched = api.fetch(video_id, languages=languages)
        except Exception as exc:
            if exc.__class__.__name__ != "NoTranscriptFound":
                raise
            fetched = api.fetch(video_id)

        return fetched.to_raw_data() if hasattr(fetched, "to_raw_data") else fetched

    # youtube-transcript-api <1.0.0
    if hasattr(YouTubeTranscriptApi, "get_transcript"):
        try:
            return YouTubeTranscriptApi.get_transcript(video_id, languages=languages)
        except Exception as exc:
            if exc.__class__.__name__ != "NoTranscriptFound":
                raise
            # Fallback: try any available transcript if preferred languages are missing.
            return YouTubeTranscriptApi.get_transcript(video_id)

    raise RuntimeError(
        "Unsupported youtube-transcript-api version: neither fetch() nor "
        "get_transcript() is available."
    )


def fetch_video_title(url: str) -> str | None:
    oembed_url = (
        "https://www.youtube.com/oembed?url="
        f"{quote_plus(url)}&format=json"
    )
    try:
        with urlopen(oembed_url, timeout=10) as response:
            payload = json.loads(response.read().decode("utf-8"))
    except Exception:
        return None
    title = payload.get("title")
    return title.strip() if isinstance(title, str) and title.strip() else None


def make_safe_filename_stem(title: str, max_chars: int, fallback: str) -> str:
    normalized = re.sub(r"\s+", " ", title).strip()
    safe = INVALID_FILENAME_CHARS_RE.sub("", normalized).strip(" .")
    if not safe:
        return fallback
    if max_chars > 0 and len(safe) > max_chars:
        safe = safe[:max_chars].rstrip(" .")
    return safe or fallback


def build_markdown(
    url: str, video_id: str, transcript: list[dict], video_title: str | None = None
) -> str:
    lines = [normalize_text(item.get("text", "")) for item in transcript]
    lines = [line for line in lines if line]

    generated_at = datetime.now(timezone.utc).isoformat()
    body = "\n".join(lines)
    title_line = f"- Video Title: {video_title}\n" if video_title else ""

    return (
        "# YouTube Transcript\n\n"
        f"- Source URL: {url}\n"
        f"- Video ID: `{video_id}`\n"
        f"{title_line}"
        f"- Generated (UTC): {generated_at}\n\n"
        "## Transcript\n\n"
        f"{body}\n"
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate a Markdown transcript from a YouTube URL or video ID."
    )
    parser.add_argument(
        "url_or_id",
        help="YouTube URL (watch/shorts/live/embed/youtu.be) or raw 11-char video ID",
    )
    parser.add_argument(
        "-o",
        "--output",
        help="Output markdown file path (default: transcripts/<video_title>.md)",
    )
    parser.add_argument(
        "--max-title-chars",
        type=int,
        default=90,
        help="Max filename length when using video title (default: 90).",
    )
    parser.add_argument(
        "--lang",
        dest="languages",
        action="append",
        default=[],
        help="Preferred transcript language code. Repeatable (example: --lang en --lang en-US).",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    url_or_id = args.url_or_id

    try:
        video_id = extract_video_id(url_or_id)
    except ValueError as exc:
        print(str(exc), file=sys.stderr)
        return 2

    source_url = (
        url_or_id
        if urlparse(url_or_id).scheme
        else f"https://www.youtube.com/watch?v={video_id}"
    )
    video_title = fetch_video_title(source_url)

    output_path = Path(args.output) if args.output else None
    if output_path is None:
        stem = make_safe_filename_stem(
            video_title or video_id,
            max_chars=args.max_title_chars,
            fallback=video_id,
        )
        output_path = Path("transcripts") / f"{stem}.md"

    output_path.parent.mkdir(parents=True, exist_ok=True)

    languages = args.languages or ["en", "en-US", "en-GB"]

    try:
        transcript = fetch_transcript(video_id, languages)
    except Exception as exc:  # pragma: no cover
        err_name = exc.__class__.__name__
        if err_name in {"TranscriptsDisabled", "NoTranscriptFound", "VideoUnavailable"}:
            print(f"Unable to fetch transcript for {video_id}: {exc}", file=sys.stderr)
            return 1
        print(f"Unexpected error while fetching transcript: {exc}", file=sys.stderr)
        return 1

    markdown = build_markdown(source_url, video_id, transcript, video_title=video_title)
    output_path.write_text(markdown, encoding="utf-8")

    print(f"Transcript saved to: {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
