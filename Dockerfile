FROM rust:1-alpine3.22 AS builder

RUN apk add --no-cache build-base cmake nodejs npm perl
WORKDIR /src
COPY . .
RUN cd apps/web && npm ci && npm run build
RUN cargo build --locked --release --package backuppo

FROM alpine:3.22 AS restic
ARG TARGETARCH
ARG RESTIC_VERSION=0.19.1
RUN apk add --no-cache bzip2 curl \
    && case "$TARGETARCH" in \
         amd64) asset="restic_${RESTIC_VERSION}_linux_amd64.bz2"; checksum="f415415624dcc452f2a02b8c33641791a8c6d6d3b65bbb3543fcf9a25151585c" ;; \
         arm64) asset="restic_${RESTIC_VERSION}_linux_arm64.bz2"; checksum="a5f64aaab53d51e311fa3829124c5b703f2d14cf187d8640b6be3b2b49376465" ;; \
         *) echo "unsupported architecture: $TARGETARCH"; exit 1 ;; \
       esac \
    && curl --fail --location --output /tmp/restic.bz2 "https://github.com/restic/restic/releases/download/v${RESTIC_VERSION}/${asset}" \
    && echo "${checksum}  /tmp/restic.bz2" | sha256sum -c - \
    && bzip2 -d /tmp/restic.bz2 \
    && chmod 755 /tmp/restic

FROM alpine:3.22

RUN apk add --no-cache ca-certificates tzdata \
    && addgroup -S backuppo \
    && adduser -S -G backuppo -h /var/lib/backuppo backuppo \
    && mkdir -p /etc/backuppo /var/lib/backuppo \
    && chown -R backuppo:backuppo /etc/backuppo /var/lib/backuppo
COPY --from=builder /src/target/release/bkpo /usr/local/bin/bkpo
COPY --from=restic /tmp/restic /usr/local/bin/restic
COPY --chown=backuppo:backuppo deploy/config/agent-docker.yaml /etc/backuppo/config.yaml

USER backuppo
VOLUME ["/etc/backuppo", "/var/lib/backuppo"]
EXPOSE 8787
ENTRYPOINT ["/usr/local/bin/bkpo"]
CMD ["daemon", "--config", "/etc/backuppo/config.yaml"]
