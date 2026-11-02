# URL Service

긴 URL을 짧은 주소로 바꿔 주는 서비스. 용어는 `GLOSSARY.md`를 따른다.

## 구성

Cargo workspace 모노레포.

| 디렉터리 | 패키지 | 역할 |
|---|---|---|
| `crates/web` | `web` | Dioxus fullstack 앱 (UI + server function) |
| `crates/core` | `url-core` | 도메인 로직. Dioxus에 의존하지 않는다 |
| `crates/storage` | `url-storage` | 저장소 구현 |

## 실행

필요한 도구: Rust(stable), `wasm32-unknown-unknown` 타깃, Dioxus CLI 0.7.10.

```sh
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli --version 0.7.10 --locked   # 또는 cargo binstall dioxus-cli@0.7.10

dx serve --package web --port 8080
```

워크스페이스 루트에서 실행하고 http://localhost:8080 을 연다. 서버 바이너리는 `public/`
디렉터리가 필요하므로 `cargo run`이 아니라 `dx serve`/`dx build`로 실행한다.

서버는 다음 환경 변수를 읽는다.

| 변수 | 기본값 | 뜻 |
|---|---|---|
| `BASE_URL` | `http://localhost:8080` | 단축 URL 앞부분. 단축 URL은 `BASE_URL/단축 코드` |
| `DATABASE_PATH` | `data/links.db` | 링크를 저장하는 SQLite 파일. 상대 경로는 `dx`를 실행한 디렉터리 기준이며, 없으면 디렉터리와 파일을 만든다 |

```sh
BASE_URL=http://localhost:8081 dx serve --package web --port 8081
```

## 테스트와 빌드 확인

```sh
cargo test --workspace
cargo check -p web --features server                               # 서버 쪽
cargo check -p web --features web --target wasm32-unknown-unknown  # 브라우저(wasm) 쪽
```

링크 생성 seam 테스트는 `crates/storage/tests/link_creation.rs`에 있다. `url-core`의 링크 생성
서비스를 실제 SQLite 저장소(테스트마다 새 in-memory DB)에 붙여 호출한다.

서버 전용 의존성(`url-core`, `url-storage` 등)은 `web`의 `server` feature에서만 켜지므로
wasm 빌드에 섞이지 않는다.
