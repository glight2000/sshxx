FROM rust:1.99.1-alpine3.24 AS backend
WORKDIR /home/rust/src
RUN apk --no-cache add musl-dev openssl-dev protoc
RUN rustup component add rustfmt
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/home/rust/src/target \
    cargo build --release --bin sshxx-server && \
    cp target/release/sshxx-server /usr/local/bin

FROM node:24.22.1-alpine3.24 AS frontend
RUN apk --no-cache add git
WORKDIR /usr/src/app
COPY . .
RUN npm install --global npm@12.2.1 && npm ci
RUN npm run build

FROM alpine:3.24.3
WORKDIR /root
COPY --from=frontend /usr/src/app/build build
COPY --from=backend /usr/local/bin/sshxx-server .
CMD ["./sshxx-server", "--listen", "::"]
