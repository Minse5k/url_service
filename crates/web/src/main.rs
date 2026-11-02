use dioxus::prelude::*;

mod input;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut original_url = use_signal(String::new);
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
            // 생성 동작은 다음 티켓(02)에서 붙인다.
            button { r#type: "button", disabled, "생성" }
        }
    }
}
