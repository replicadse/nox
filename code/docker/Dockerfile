FROM lukemathwalker/cargo-chef:latest-rust-1.75.0 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG BIN
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --package $BIN

FROM ubuntu:22.04 AS runtime
ARG BIN
WORKDIR /app
RUN apt-get update -y && apt-get upgrade -y && apt-get install -y libpq-dev
COPY --from=builder /app/target/release/$BIN /usr/local/bin/app
ENTRYPOINT ["/usr/local/bin/app"]
