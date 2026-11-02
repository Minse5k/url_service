use dioxus::prelude::*;

mod copy;
mod input;
#[cfg(feature = "server")]
mod server;

/// 링크 생성 결과로 화면에 보여 줄 값. 클라이언트와 서버가 함께 쓴다.
#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct CreatedLink {
    pub short_url: String,
    pub original_url: String,
}

/// 링크 생성 server function. `url-core`의 링크 생성 서비스를 호출만 한다.
/// 자세한 오류(저장소 내부 정보)는 서버 로그에만 남기고 브라우저에는 일반 메시지만 보낸다.
#[post("/api/links", creator: server::Creator)]
async fn create_link(original_url: String) -> Result<CreatedLink, ServerFnError> {
    let created = creator
        .create_link(url_core::OriginalUrl::new(original_url))
        .await
        .map_err(|e| {
            dioxus::logger::tracing::error!("link creation failed: {e}");
            ServerFnError::new("internal error")
        })?;
    Ok(CreatedLink {
        short_url: created.short_url.as_str().to_owned(),
        original_url: created.original_url.as_str().to_owned(),
    })
}

fn main() {
    #[cfg(not(feature = "server"))]
    dioxus::launch(App);

    #[cfg(feature = "server")]
    dioxus::serve(|| async move {
        let creator = server::creator_from_env().await?;
        Ok(dioxus::server::router(App).layer(dioxus::server::axum::Extension(creator)))
    });
}

#[component]
fn App() -> Element {
    let mut original_url = use_signal(String::new);
    let mut created = use_signal(|| None::<CreatedLink>);
    let mut error = use_signal(|| None::<String>);
    // 요청이 진행 중이면 다시 누를 수 없다(중복 생성과 늦은 응답의 덮어쓰기 방지).
    let mut pending = use_signal(|| false);
    let disabled = pending() || !input::can_create(&original_url.read());

    rsx! {
        document::Title { "URL Service" }
        main {
            h1 { "URL 단축" }
            input {
                r#type: "url",
                placeholder: "원본 URL을 입력하세요",
                aria_label: "원본 URL",
                value: "{original_url}",
                oninput: move |e| original_url.set(e.value()),
            }
            button {
                r#type: "button",
                disabled,
                onclick: move |_| async move {
                    pending.set(true);
                    let result = create_link(original_url.read().trim().to_owned()).await;
                    pending.set(false);
                    match result {
                        Ok(link) => {
                            created.set(Some(link));
                            error.set(None);
                        }
                        Err(_) => {
                            created.set(None);
                            error.set(Some("링크를 만들지 못했습니다. 잠시 후 다시 시도해 주세요.".into()));
                        }
                    }
                },
                "생성"
            }
            if let Some(link) = created() {
                section { aria_label: "생성된 링크",
                    p {
                        "단축 URL: "
                        a { href: "{link.short_url}", "{link.short_url}" }
                        " "
                        CopyButton { key: "{link.short_url}", text: link.short_url.clone() }
                    }
                    p { "원본 URL: {link.original_url}" }
                }
            }
            if let Some(message) = error() {
                p { role: "alert", "{message}" }
            }
        }
    }
}

/// 누르면 `text`를 클립보드에 복사하고, `copy::COPIED_FOR` 동안 "복사됨"을 보여 준다.
#[component]
fn CopyButton(text: String) -> Element {
    let mut label = use_signal(copy::CopyLabel::default);

    rsx! {
        button {
            r#type: "button",
            onclick: move |_| {
                let text = text.clone();
                async move {
                    let timer = match write_clipboard(text).await {
                        Ok(()) => label.write().copied(),
                        Err(_) => label.write().failed(),
                    };
                    sleep(copy::COPIED_FOR).await;
                    label.write().expire(timer);
                }
            },
            "{label.read().text()}"
        }
    }
}

/// 브라우저 클립보드에 `text`를 쓴다. 값은 문자열 조립 대신 채널로 넘긴다.
async fn write_clipboard(text: String) -> Result<(), document::EvalError> {
    let eval = document::eval(
        "const text = await dioxus.recv(); await navigator.clipboard.writeText(text); return true;",
    );
    eval.send(text)?;
    eval.await.map(|_| ())
}

/// 브라우저의 `setTimeout`으로 기다린다. wasm에서도 동작하고 의존성이 늘지 않는다.
async fn sleep(duration: std::time::Duration) {
    let js = format!(
        "await new Promise(r => setTimeout(r, {})); return true;",
        duration.as_millis()
    );
    let _ = document::eval(&js).await;
}
