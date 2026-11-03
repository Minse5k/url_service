# 숫자형 단축 URL 서비스

## 배포 URL

- 서비스 URL: (배포 후 기입)
- 테스트 단축 URL: (배포 후 기입)
- 테스트 원본 URL: (배포 후 기입)

긴 URL을 `/123`처럼 숫자로만 된 주소로 줄이는 서비스입니다. 로그인 없이 누구나 만들 수 있고, 한 번 만든 링크는 바뀌지 않습니다.

- 화면·공개 접점: Cloudflare Worker (정적 자산 + 프록시)
- 백엔드(생성 API·리디렉션): Render에서 실행하는 Rust 서버 (Dioxus 0.7 fullstack, axum)
- DB: Render PostgreSQL

> 과제는 백엔드·DB를 Railway에 두도록 안내하지만, Railway가 새 계정의 체험판을 중단해 유료 플랜 없이는 배포할 수 없어 Render로 대신 배포했습니다. 백엔드는 같은 Docker 이미지라 Railway로 옮길 때는 `render.yaml` 대신 Railway 서비스와 변수 두 개(`DATABASE_URL`, `BASE_URL`)만 설정하면 됩니다.

## 구조

```
브라우저 ──HTTPS──▶ Cloudflare Worker (공개 주소)
                     ├─ /, /assets/*      → 정적 자산(미리 렌더링한 HTML + wasm)
                     └─ /숫자, /api/*      → Render 백엔드로 프록시  (302는 그대로 전달)
                                             └─ PostgreSQL (Render)
```

| 경로 | 설명 |
|---|---|
| `crates/core` (`url-core`) | 도메인 로직: 원본 URL 검증, 링크 번호, 링크 생성. DB·Dioxus에 의존하지 않음 |
| `crates/storage` (`url-storage`) | PostgreSQL 저장소(sqlx)와 마이그레이션(`migrations/`) |
| `crates/web` | Dioxus fullstack 앱: 화면, 생성 server function, `/숫자` 리디렉션 |
| `cloudflare/` | Worker 코드(`src/worker.js`)와 `wrangler.toml` |
| `scripts/build-cloudflare.sh` | Cloudflare에 올릴 `cloudflare/public/`을 만드는 스크립트 |
| `Dockerfile`, `render.yaml` | Render 백엔드·DB 빌드·배포 설정(Blueprint) |

## 동작 규칙

### 링크 번호 발급
- 번호는 PostgreSQL `links.id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY`가 발급합니다.
- 서버는 `INSERT ... RETURNING id` 한 문장으로 저장과 발급을 함께 합니다. "최대값 조회 후 +1" 같은 단계가 없어 경쟁 상태가 생기지 않습니다.
- 시퀀스는 트랜잭션과 무관하게 한 번 나간 값을 다시 주지 않으므로 **동시 요청에서도 번호가 겹치지 않고, 번호는 재사용되지 않습니다**. 실패한 INSERT 때문에 번호에 빈칸이 생길 수는 있습니다.
- `GENERATED ALWAYS`이므로 클라이언트나 SQL이 번호를 직접 지정할 수 없습니다(지정하면 DB가 거부).
- 경로는 1 이상의 ASCII 숫자만 받습니다. `/007`처럼 앞자리가 0이면 다른 번호의 별칭이 되므로 404로 처리합니다.

### 같은 URL을 다시 입력하면
**매번 새 번호를 발급합니다.** 원본 URL에는 UNIQUE 제약이 없습니다. 이유:
- 링크마다 "누가 언제 만든 링크인지"가 독립적이어서, 나중에 통계·신고 같은 기능을 링크 단위로 붙이기 쉽습니다.
- 기존 번호를 찾아 돌려주려면 정규화 기준(대소문자, 끝 `/`, 쿼리 순서 등)을 정해야 하는데, 이 기준이 원본 URL의 의미를 바꿀 위험이 있습니다.

### 링크 불변성
- 수정·삭제·만료 기능이 없습니다. 저장소 trait(`LinkStore`)에도 `insert`와 `find`만 있습니다.
- `/숫자`와 `/api/links/숫자`로 오는 `PUT`·`PATCH`·`DELETE`·`POST` 등 변경 요청은 서버가 `405 Method Not Allowed`(`Allow: GET, HEAD`)로 거부합니다.
- DB 트리거가 `links`의 `UPDATE`·`DELETE`·`TRUNCATE`를 거부합니다. 코드를 우회한 SQL로도 바뀌지 않습니다.

### 리디렉션
- `GET /숫자`는 `302 Found`와 `Location: <저장된 원본 URL>`을 돌려줍니다. 원본 URL의 경로·쿼리·`#fragment`가 그대로 유지됩니다.
- 302를 쓰는 이유: 브라우저가 영구 캐시하지 않아 모든 방문이 서버를 거칩니다. `Cache-Control: no-store`도 함께 보냅니다.
- 없는 번호나 숫자가 아닌 경로는 `404`와 안내 페이지("존재하지 않는 단축 주소입니다…")를 보여 줍니다.

### 입력 검증 (서버)
- 앞뒤 공백을 제거한 뒤 `url` 크레이트로 파싱합니다.
- `http://`·`https://`만 허용(`ftp:`, `javascript:`, `mailto:` 등 거부), 호스트 필수, 최대 2048자.
- 이 서비스 자신(`BASE_URL`의 호스트)을 가리키는 URL은 거부합니다(단축 URL의 재단축 방지).
- 실패하면 `400`과 한국어 안내 메시지를 돌려주고, 화면에 그대로 표시합니다.
- DB에도 `CHECK (original_url ~ '^https?://' AND char_length(original_url) <= 2048)`이 있습니다.

## 환경 변수

| 이름 | 어디에 | 설명 |
|---|---|---|
| `DATABASE_URL` | Render 백엔드 | PostgreSQL 접속 주소. **비밀값**. `render.yaml`의 `fromDatabase`로 Render가 주입(값을 저장소에 두지 않음) |
| `BASE_URL` | Render 백엔드 | 단축 URL 앞부분 = 공개 서비스 주소(Cloudflare). 예: `https://url-service.<계정>.workers.dev` |
| `IP`, `PORT` | Render 백엔드 | 바인딩 주소. 이미지 기본값 `IP=0.0.0.0`, `PORT`는 Render가 주입 |
| `BACKEND_URL` | Cloudflare Worker | Render 백엔드 공개 주소. 비밀값 아님, `cloudflare/wrangler.toml`의 `[vars]` |
| `TEST_DATABASE_URL` | 로컬 테스트 | 설정하면 Postgres 통합 테스트 실행 |

예시는 [`.env.example`](.env.example)에 있습니다. DB 비밀번호나 서비스 토큰은 저장소·브라우저 코드 어디에도 넣지 않습니다. 브라우저 코드는 같은 출처의 `/api/links`만 호출하고, 백엔드 주소조차 알지 못합니다.

## 로컬 실행

필요한 도구: Rust(1.85+, edition 2024), `wasm32-unknown-unknown` 타깃, Dioxus CLI 0.7.10, Docker(Postgres용), Node.js(Worker 로컬 실행용).

```bash
rustup target add wasm32-unknown-unknown
cargo binstall dioxus-cli@0.7.10   # 또는 cargo install dioxus-cli --version 0.7.10 --locked
```

1. PostgreSQL 실행 (마이그레이션은 서버가 시작할 때 자동 적용)

```bash
docker run -d --name url-pg -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=url_service -p 55432:5432 postgres:17
```

2. 앱 실행 후 <http://localhost:8090> 접속

```bash
DATABASE_URL=postgres://postgres:postgres@localhost:55432/url_service BASE_URL=http://localhost:8090 dx serve --package web --platform web --port 8090
```

3. 테스트

```bash
TEST_DATABASE_URL=postgres://postgres:postgres@localhost:55432/url_service cargo test --workspace --features web/server
```

4. (선택) Cloudflare Worker까지 로컬에서 실행: 2번 서버를 띄운 상태에서

```bash
./scripts/build-cloudflare.sh
cd cloudflare && npm install && npx wrangler dev --port 8787 --var BACKEND_URL:http://localhost:8090
```

## 배포

### 1. Render (백엔드 + DB)

1. Render 대시보드에서 **New > Blueprint**를 누르고 이 GitHub 저장소를 연결합니다. 루트의 `render.yaml`이 두 리소스를 만듭니다.
   - `url-service-db`: PostgreSQL (free, singapore)
   - `url-service-api`: `Dockerfile`로 빌드하는 웹 서비스 (free, singapore, 헬스체크 `/`)
2. Dockerfile은 `dx bundle --package web --web --release`로 서버 바이너리와 `public/`을 만들고, 슬림 이미지에서 `./server`를 실행합니다. 마이그레이션은 서버가 시작할 때 적용합니다.
3. 환경 변수
   - `DATABASE_URL`: `fromDatabase`로 Render가 DB의 내부 접속 주소를 주입합니다. 값을 복사하거나 저장소에 넣지 않습니다.
   - `BASE_URL`: Cloudflare 공개 주소. `render.yaml`에 적혀 있습니다(비밀값 아님).
4. 배포가 끝나면 서비스 주소(`https://<서비스>.onrender.com`)를 Cloudflare의 `BACKEND_URL`에 넣습니다.

무료 플랜 참고: 웹 서비스는 15분 동안 요청이 없으면 잠들고, 다음 요청 때 깨어나느라 수십 초 걸릴 수 있습니다. 무료 PostgreSQL은 생성 후 30일이 지나면 만료됩니다.

### 2. Cloudflare (화면 + 공개 접점)

1. `cloudflare/wrangler.toml`의 `BACKEND_URL`을 Render 서비스 주소(`https://<서비스>.onrender.com`)로 설정합니다.
2. 정적 파일을 만들고 배포합니다.

```bash
./scripts/build-cloudflare.sh
cd cloudflare && npm install && npx wrangler login && npx wrangler deploy
```

`build-cloudflare.sh`가 하는 일: Dioxus fullstack 클라이언트는 서버가 렌더링한 HTML(하이드레이션 데이터 포함)에서 시작해야 합니다. 그래서 릴리스 번들의 서버를 접속되지 않는 더미 `DATABASE_URL`로 잠깐 띄워 `/`를 한 번 렌더링하고, 그 HTML과 wasm/js 자산을 `cloudflare/public/`에 넣습니다. 화면 렌더링에는 DB가 필요 없습니다.

### 라우팅

`cloudflare/src/worker.js`(`run_worker_first = true`로 모든 요청을 Worker가 먼저 받음):

| 요청 경로 | 처리 |
|---|---|
| `^/\d+$` (예: `/123`) | Render로 프록시, `redirect: "manual"`이라 302·`Location`을 그대로 브라우저에 전달 |
| `/api/*` (예: `POST /api/links`) | Render로 프록시 |
| 그 밖 (`/`, `/assets/*`) | 정적 자산. 없는 경로는 `404.html` |

## 검증

### 자동 테스트 (로컬에서 실행, 통과)

`cargo test --workspace --features web/server` (Postgres 테스트는 `TEST_DATABASE_URL` 설정 시)

| 테스트 | 확인 내용 |
|---|---|
| `url-core` 단위 테스트 8개 | http/https만 허용, 공백 제거, 경로·쿼리·fragment 유지, 2048자 제한, 서비스 자신 거부, 경로 숫자 규칙(`0`, `007`, `-1`, `1a`, 범위 초과 거부) |
| `url-storage` Postgres 테스트 6개 | 저장·조회, 같은 URL → 새 번호, 없는 번호 조회, **동시 100건 생성 시 번호 중복 없음**, DB가 `UPDATE`·`DELETE`·번호 직접 지정 거부, DB가 `javascript:` 거부 |
| `web` 테스트 5개 | 302 + `Location` 유지, 없는/잘못된 번호 404 안내, `PUT`·`PATCH`·`DELETE`·`POST` 405 후 링크 그대로, 복사 버튼 라벨 상태 |

### 시나리오 (로컬 Worker → 로컬 백엔드, curl로 확인)

| 시나리오 | 요청 | 결과 |
|---|---|---|
| 화면 | `GET /` | 200, 하이드레이션 데이터 포함 HTML. 브라우저에서 입력 → 생성 → 단축 URL·복사 버튼 표시 |
| 생성 | `POST /api/links {"original_url":"https://example.org/a/b?x=1#frag"}` | 200, `{"short_url":".../107", ...}` |
| 잘못된 스킴 | `POST /api/links {"original_url":"ftp://example.org"}` | 400, "http:// 또는 https:// 로 시작하는 URL만 사용할 수 있습니다." |
| 리디렉션 | `GET /1` | 302, `Location: <원본 URL>`, `Cache-Control: no-store` |
| 없는 번호 | `GET /999999` | 404 안내 페이지 |
| 변경 요청 | `DELETE /1`, `PUT /api/links/1` | 405 |

### 배포 환경에서 재현하는 방법

```bash
SVC=https://<서비스 URL>
curl -s -X POST -H 'content-type: application/json' -d '{"original_url":"https://example.org/a/b?x=1#frag"}' $SVC/api/links
curl -sI $SVC/<번호>            # 302 + Location
curl -sI $SVC/999999999         # 404
curl -s -o /dev/null -w '%{http_code}\n' -X DELETE $SVC/<번호>   # 405
```
