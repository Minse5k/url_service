#!/usr/bin/env bash
# Cloudflare에 올릴 정적 파일을 `cloudflare/public/`에 만든다.
#
# Dioxus fullstack 클라이언트는 서버가 렌더링한 HTML(하이드레이션 데이터 포함)에서 시작해야 한다.
# 그래서 릴리스 번들의 서버를 잠깐 띄워 `/`를 한 번 렌더링하고, 그 HTML을 index.html로 저장한다.
# 화면 렌더링에는 DB가 필요 없으므로 접속되지 않는 더미 DATABASE_URL로 띄운다(실제 비밀값 불필요).
set -euo pipefail

cd "$(dirname "$0")/.."
BUNDLE=target/dx/web/release/web
OUT=cloudflare/public
PORT="${PRERENDER_PORT:-8399}"

dx bundle --package web --web --release

rm -rf "$OUT"
cp -R "$BUNDLE/public" "$OUT"
cp cloudflare/404.html "$OUT/404.html"

DATABASE_URL=postgres://prerender@127.0.0.1:1/prerender IP=127.0.0.1 PORT="$PORT" "$BUNDLE/server" >/dev/null 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT

for _ in $(seq 1 50); do
  if curl -fsS "http://127.0.0.1:$PORT/" -o "$OUT/index.html" 2>/dev/null; then
    break
  fi
  sleep 0.2
done
grep -q initial_dioxus_hydration_data "$OUT/index.html" || { echo "prerender failed" >&2; exit 1; }
echo "built $OUT"
