//! 서버 전용: 환경 변수로 상태를 만들고, `/숫자` 리디렉션과 변경 요청 거부 라우트를 제공한다.

use std::sync::Arc;
use std::time::Duration;

use dioxus::server::axum::{
    Router,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{any, get},
};
use url_core::{LinkNumber, LinkStore};
use url_storage::PgLinkStore;

/// server function과 리디렉션 핸들러가 함께 쓰는 상태.
pub struct AppState<S> {
    pub store: S,
    /// 단축 URL 앞부분. 공개 접점(Cloudflare) 주소를 넣는다.
    pub base_url: String,
}

pub type SharedState = Arc<AppState<PgLinkStore>>;

const DEFAULT_BASE_URL: &str = "http://localhost:8080";

/// `DATABASE_URL`(필수)과 `BASE_URL`(기본 `http://localhost:8080`)로 상태를 만든다.
///
/// 접속은 첫 요청 때 맺고, 마이그레이션은 백그라운드에서 성공할 때까지 재시도한다.
/// 그래서 DB가 잠시 늦게 떠도 서버는 먼저 뜬다(화면 사전 렌더링도 DB 없이 가능).
pub fn state_from_env() -> std::io::Result<SharedState> {
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| std::io::Error::other("DATABASE_URL environment variable is not set"))?;
    let base_url = std::env::var("BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.into());
    let store = PgLinkStore::connect_lazy(&database_url).map_err(std::io::Error::other)?;

    let migrating = store.clone();
    tokio::spawn(async move {
        loop {
            match migrating.migrate().await {
                Ok(()) => break tracing::info!("database migrations applied"),
                Err(e) => tracing::error!("database migration failed, retrying in 5s: {e}"),
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });

    Ok(Arc::new(AppState { store, base_url }))
}

/// `/숫자` 리디렉션과, 링크를 바꾸려는 요청을 거부하는 라우트.
///
/// 이 서비스에는 링크 수정·삭제 API가 없다. 그래도 `/숫자`나 `/api/links/숫자`로 들어오는
/// `PUT`/`PATCH`/`DELETE`/`POST` 같은 요청은 명시적으로 `405 Method Not Allowed`로 거부한다.
pub fn link_routes<S: LinkStore + 'static>(state: Arc<AppState<S>>) -> Router {
    Router::new()
        .route("/{number}", get(redirect::<S>).fallback(reject_change))
        .route("/api/links/{number}", any(reject_change))
        .with_state(state)
}

/// `GET /숫자`: 저장된 원본 URL로 `302 Found` 리디렉션한다.
///
/// `Location`에는 저장한 원본 URL을 그대로 넣으므로 경로·쿼리·프래그먼트(`#...`)가 유지된다.
async fn redirect<S: LinkStore>(
    State(state): State<Arc<AppState<S>>>,
    Path(segment): Path<String>,
) -> Response {
    let Some(number) = LinkNumber::parse_path(&segment) else {
        return not_found();
    };
    match state.store.find(number).await {
        Ok(Some(original_url)) => match HeaderValue::from_str(original_url.as_str()) {
            Ok(location) => (
                StatusCode::FOUND,
                [(header::LOCATION, location), (header::CACHE_CONTROL, HeaderValue::from_static("no-store"))],
            )
                .into_response(),
            Err(_) => server_error(),
        },
        Ok(None) => not_found(),
        Err(e) => {
            tracing::error!("failed to look up link {number}: {e}");
            server_error()
        }
    }
}

async fn reject_change() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::ALLOW, "GET, HEAD")],
        "Links are immutable: an issued number can never be changed or deleted.\n\
         링크는 생성 후 변경하거나 삭제할 수 없습니다.\n",
    )
        .into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, page("링크를 찾을 수 없습니다", "존재하지 않는 단축 주소입니다. 주소의 숫자를 다시 확인해 주세요.")).into_response()
}

fn server_error() -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, page("일시적인 오류", "잠시 후 다시 시도해 주세요.")).into_response()
}

fn page(title: &str, message: &str) -> Html<String> {
    Html(format!(
        r#"<!doctype html>
<html lang="ko">
<head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>{title}</title>
<style>body{{font-family:system-ui,sans-serif;max-width:32rem;margin:15vh auto;padding:0 1rem;color:#222}}a{{color:#2557d6}}</style></head>
<body><h1>{title}</h1><p>{message}</p><p><a href="/">새 단축 URL 만들기</a></p></body>
</html>
"#
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use dioxus::server::axum::body::Body;
    use dioxus::server::axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use url_core::{OriginalUrl, StoreError, create_link};

    use super::*;

    /// 메모리 저장소: 번호는 1부터 차례로 발급한다.
    #[derive(Default)]
    struct MemoryStore(Mutex<Vec<OriginalUrl>>);

    impl LinkStore for MemoryStore {
        async fn insert(&self, url: &OriginalUrl) -> Result<LinkNumber, StoreError> {
            let mut links = self.0.lock().unwrap();
            links.push(url.clone());
            Ok(LinkNumber::new(links.len() as i64).unwrap())
        }
        async fn find(&self, number: LinkNumber) -> Result<Option<OriginalUrl>, StoreError> {
            Ok(self.0.lock().unwrap().get(number.get() as usize - 1).cloned())
        }
    }

    fn state() -> Arc<AppState<MemoryStore>> {
        Arc::new(AppState { store: MemoryStore::default(), base_url: "https://s.example".into() })
    }

    async fn send(state: &Arc<AppState<MemoryStore>>, method: &str, uri: &str) -> Response {
        let request = Request::builder().method(method).uri(uri).body(Body::empty()).unwrap();
        link_routes(state.clone()).oneshot(request).await.unwrap()
    }

    async fn body(response: Response) -> String {
        String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    }

    #[tokio::test]
    async fn redirects_to_original_url_keeping_path_query_and_fragment() {
        let state = state();
        let created =
            create_link(&state.store, &state.base_url, "https://example.org/a/b?x=1&y=2#sec").await.unwrap();
        assert_eq!(created.short_url, "https://s.example/1");

        let response = send(&state, "GET", "/1").await;
        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(response.headers()[header::LOCATION], "https://example.org/a/b?x=1&y=2#sec");
    }

    #[tokio::test]
    async fn unknown_or_non_numeric_paths_are_404_with_a_message() {
        let state = state();
        for path in ["/1", "/999999", "/0", "/007", "/abc", "/12a", "/99999999999999999999"] {
            let response = send(&state, "GET", path).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
            assert!(body(response).await.contains("존재하지 않는 단축 주소"), "{path}");
        }
    }

    #[tokio::test]
    async fn requests_to_change_a_link_are_rejected_and_the_link_is_unchanged() {
        let state = state();
        create_link(&state.store, &state.base_url, "https://example.org/keep").await.unwrap();

        for method in ["PUT", "PATCH", "DELETE", "POST"] {
            for path in ["/1", "/api/links/1"] {
                let response = send(&state, method, path).await;
                assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED, "{method} {path}");
            }
        }

        let response = send(&state, "GET", "/1").await;
        assert_eq!(response.headers()[header::LOCATION], "https://example.org/keep");
    }
}
