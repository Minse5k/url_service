use dioxus::prelude::*;

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
#[post("/api/links", creator: server::Creator)]
async fn create_link(original_url: String) -> Result<CreatedLink, ServerFnError> {
    let created = creator
        .create_link(url_core::OriginalUrl::new(original_url))
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
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
    let disabled = !input::can_create(&original_url.read());

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
                    match create_link(original_url()).await {
                        Ok(link) => {
                            created.set(Some(link));
                            error.set(None);
                        }
                        Err(e) => error.set(Some(format!("링크를 만들지 못했습니다: {e}"))),
                    }
                },
                "생성"
            }
            if let Some(link) = created() {
                section { aria_label: "생성된 링크",
                    p {
                        "단축 URL: "
                        a { href: "{link.short_url}", "{link.short_url}" }
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
