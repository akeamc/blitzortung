FROM clux/muslrust:1.67.0 as build-env
WORKDIR /app
COPY . .
RUN cargo build --release --bin lightningalert

FROM gcr.io/distroless/static:nonroot
COPY --from=build-env --chown=nonroot:nonroot /app/target/x86_64-unknown-linux-musl/release/lightningalert /
ENTRYPOINT ["/lightningalert"]
