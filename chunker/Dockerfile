FROM debian:bookworm-slim

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates ffmpeg \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY chunker/dist/vid-archi-chunker /app/vid-archi-chunker
COPY config/video_policy.json /app/config/video_policy.json

ENV VIDEO_POLICY_FILE=/app/config/video_policy.json

ENTRYPOINT ["/app/vid-archi-chunker"]
