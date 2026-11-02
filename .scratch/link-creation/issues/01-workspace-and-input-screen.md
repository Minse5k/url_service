# 01: 워크스페이스와 빈 입력 화면

**What to build:** 로그인 없이 서비스에 들어오면 원본 URL 입력창과 "생성" 버튼이 있는 화면이 보인다. 입력이 비어 있으면 "생성" 버튼은 비활성화된다. 이 티켓에서 생성 버튼은 아직 아무 동작도 하지 않는다. 이후 티켓의 토대로 Cargo workspace 모노레포(`web`: Dioxus fullstack 앱, `core`: Dioxus에 의존하지 않는 도메인 로직, `storage`: 저장소 구현)를 구성한다.

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 루트에 Cargo workspace가 있고 `web`, `core`, `storage` 크레이트가 멤버로 등록되어 있다
- [x] `core`는 Dioxus에 의존하지 않는다
- [x] Dioxus fullstack 최신 안정 버전으로 앱을 실행하면 `http://localhost:8080`에서 입력 화면이 열린다
- [x] 원본 URL 입력창과 "생성" 버튼이 보인다
- [x] 입력이 비어 있으면(공백만 있는 경우 포함) "생성" 버튼이 비활성화되고, 값을 입력하면 활성화된다
- [x] 워크스페이스 전체가 빌드되고 `cargo test`가 통과한다
