// 공개 접점(Cloudflare Worker).
// - `/숫자`와 `/api/*`는 Render의 Rust 백엔드로 그대로 넘긴다. 302는 따라가지 않고 브라우저에 그대로 돌려준다.
// - 그 밖의 경로(화면 HTML, wasm/js)는 정적 자산으로 응답한다.
// 백엔드 주소는 wrangler.toml의 `BACKEND_URL` 변수로만 받는다. 이 Worker에는 비밀값이 없다.

const BACKEND_PATH = /^\/(?:\d+|api\/.*)$/;

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (!BACKEND_PATH.test(url.pathname)) {
      return env.ASSETS.fetch(request);
    }

    const target = new URL(url.pathname + url.search, env.BACKEND_URL);
    const headers = new Headers(request.headers);
    headers.set("X-Forwarded-Host", url.host);
    headers.set("X-Forwarded-Proto", url.protocol.replace(":", ""));
    return fetch(target, {
      method: request.method,
      headers,
      body: request.body,
      redirect: "manual",
    });
  },
};
