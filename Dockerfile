FROM rust:1-alpine3.22 AS builder

RUN apk add --no-cache build-base cmake perl
WORKDIR /src
COPY . .
RUN cargo build --locked --release --package backuppo

FROM alpine:3.22

RUN apk add --no-cache ca-certificates restic tzdata \
    && addgroup -S backuppo \
    && adduser -S -G backuppo -h /var/lib/backuppo backuppo \
    && mkdir -p /etc/backuppo /var/lib/backuppo \
    && chown -R backuppo:backuppo /var/lib/backuppo
COPY --from=builder /src/target/release/bkpo /usr/local/bin/bkpo

USER backuppo
VOLUME ["/var/lib/backuppo"]
ENTRYPOINT ["/usr/local/bin/bkpo"]
CMD ["daemon", "--config", "/etc/backuppo/config.yaml"]
