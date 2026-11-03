# Render 백엔드 이미지: 링크 생성 API(`/api/links`)와 `/숫자` 리디렉션을 제공한다.
FROM rust:1.99-trixie AS build
RUN rustup target add wasm32-unknown-unknown \
 && curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
 && cargo binstall -y dioxus-cli@0.7.10
WORKDIR /app
COPY . .
RUN dx bundle --package web --web --release

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /srv
COPY --from=build /app/target/dx/web/release/web/ ./
# Render가 PORT를 주입한다. DATABASE_URL·BASE_URL은 Render 환경 변수로 설정한다(이미지에 넣지 않음).
ENV IP=0.0.0.0 PORT=8080
EXPOSE 8080
CMD ["./server"]
