use dioxus::prelude::*;

mod copy;
#[cfg(feature = "server")]
mod server;

/// 링크 생성 결과로 화면에 보여 줄 값. 클라이언트와 서버가 함께 쓴다.
#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct CreatedLink {
    pub short_url: String,
    pub original_url: String,
}

/// 링크 생성 server function: 원본 URL을 서버에서 검증하고, DB가 발급한 새 링크 번호로 단축 URL을 만든다.
///
/// 잘못된 URL은 `400`과 사용자에게 보여 줄 안내를 돌려준다. DB 오류의 상세는 서버 로그에만 남긴다.
#[post("/api/links", state: dioxus::server::axum::Extension<server::SharedState>)]
async fn create_link(original_url: String) -> Result<CreatedLink, ServerFnError> {
    use url_core::CreateLinkError;

    match url_core::create_link(&state.store, &state.base_url, &original_url).await {
        Ok(created) => Ok(CreatedLink {
            short_url: created.short_url,
            original_url: created.original_url.as_str().to_owned(),
        }),
        Err(CreateLinkError::Invalid(reason)) => {
            Err(ServerFnError::ServerError { message: reason.to_string(), code: 400, details: None })
        }
        Err(CreateLinkError::Store(e)) => {
            tracing::error!("failed to create link: {e}");
            Err(ServerFnError::ServerError {
                message: "internal error".into(),
                code: 500,
                details: None,
            })
        }
    }
}

fn main() {
    #[cfg(not(feature = "server"))]
    dioxus::launch(App);

    #[cfg(feature = "server")]
    dioxus::serve(|| async move {
        let state = server::state_from_env()?;
        Ok(dioxus::server::router(App)
            .merge(server::link_routes(state.clone()))
            .layer(dioxus::server::axum::Extension(state)))
    });
}

/// 실패한 생성 요청을 사용자에게 보여 줄 문장으로 바꾼다.
fn error_message(error: &ServerFnError) -> String {
    match error {
        ServerFnError::ServerError { message, code: 400, .. } => message.clone(),
        _ => "링크를 만들지 못했습니다. 잠시 후 다시 시도해 주세요.".into(),
    }
}

#[component]
fn App() -> Element {
    let mut original_url = use_signal(String::new);
    let mut created = use_signal(|| None::<CreatedLink>);
    let mut error = use_signal(|| None::<String>);
    let mut pending = use_signal(|| false);
    let disabled = pending() || original_url.read().trim().is_empty();

    let submit = move || async move {
        if pending() {
            return;
        }
        pending.set(true);
        let result = create_link(original_url.read().trim().to_owned()).await;
        pending.set(false);
        match result {
            Ok(link) => {
                created.set(Some(link));
                error.set(None);
            }
            Err(e) => {
                created.set(None);
                error.set(Some(error_message(&e)));
            }
        }
    };

    rsx! {
        document::Title { "숫자형 단축 URL" }
        document::Style { {STYLE} }
        main {
            h1 { "숫자형 단축 URL" }
            p { class: "hint", "로그인 없이 긴 URL을 /123 같은 숫자 주소로 줄입니다. 한 번 만든 주소는 바뀌지 않습니다." }
            form {
                onsubmit: move |e| async move {
                    e.prevent_default();
                    if !disabled {
                        submit().await;
                    }
                },
                input {
                    r#type: "url",
                    placeholder: "https://example.org/긴/주소?query#fragment",
                    aria_label: "원본 URL",
                    value: "{original_url}",
                    oninput: move |e| original_url.set(e.value()),
                }
                button { r#type: "submit", disabled, if pending() { "생성 중…" } else { "생성" } }
            }
            if let Some(message) = error() {
                p { class: "error", role: "alert", "{message}" }
            }
            if let Some(link) = created() {
                section { class: "result", aria_label: "생성된 단축 URL",
                    div { class: "short",
                        a { href: "{link.short_url}", target: "_blank", rel: "noopener", "{link.short_url}" }
                        CopyButton { key: "{link.short_url}", text: link.short_url.clone() }
                    }
                    p { class: "original", "원본 URL: {link.original_url}" }
                }
            }
        }
    }
}

/// 누르면 `text`를 클립보드에 복사하고, `copy::SHOW_FOR` 동안 "복사됨"(실패하면 "복사 실패")을 보여 준다.
#[component]
fn CopyButton(text: String) -> Element {
    let mut label = use_signal(copy::CopyLabel::default);

    rsx! {
        button {
            r#type: "button",
            onclick: move |_| {
                let text = text.clone();
                async move {
                    let timer = if write_clipboard(text).await.is_ok() {
                        label.write().copied()
                    } else {
                        label.write().failed()
                    };
                    sleep(copy::SHOW_FOR).await;
                    label.write().expire(timer);
                }
            },
            "{label.read().text()}"
        }
    }
}

/// 브라우저 클립보드에 `text`를 쓴다. 값은 스크립트 문자열에 끼우지 않고 채널로 넘긴다.
async fn write_clipboard(text: String) -> Result<(), document::EvalError> {
    let eval = document::eval(
        "const text = await dioxus.recv(); await navigator.clipboard.writeText(text); return true;",
    );
    eval.send(text)?;
    eval.await.map(|_| ())
}

/// 브라우저의 `setTimeout`으로 기다린다. wasm에서 동작하고 의존성이 늘지 않는다.
async fn sleep(duration: std::time::Duration) {
    let js = format!("await new Promise(r => setTimeout(r, {})); return true;", duration.as_millis());
    let _ = document::eval(&js).await;
}

const STYLE: &str = r#"
:root { color-scheme: light dark; font-family: system-ui, sans-serif; }
body { margin: 0; }
main { max-width: 40rem; margin: 10vh auto; padding: 0 1rem; }
h1 { margin-bottom: .25rem; }
.hint { color: GrayText; margin-top: 0; }
form { display: flex; gap: .5rem; margin: 1.5rem 0 1rem; }
input { flex: 1; min-width: 0; padding: .6rem .7rem; font-size: 1rem; }
button { padding: .6rem 1rem; font-size: 1rem; cursor: pointer; }
button:disabled { cursor: not-allowed; }
.error { color: #c62828; }
.result { padding: 1rem; border: 1px solid GrayText; border-radius: .5rem; }
.short { display: flex; gap: .75rem; align-items: center; flex-wrap: wrap; font-size: 1.25rem; }
.original { color: GrayText; word-break: break-all; margin-bottom: 0; }
"#;
