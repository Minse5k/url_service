//! 링크 생성 seam 테스트: `url-core`의 링크 생성 서비스를 실제 SQLite 저장소에 붙여 호출한다.
//! 반환된 단축 URL과 저장된 링크만 본다.

use std::collections::VecDeque;
use std::sync::Mutex;

use url_core::{
    LinkCreationError, LinkCreator, LinkStore, OriginalUrl, RandomShortCodeGenerator, ShortCode,
    ShortCodeGenerator,
};
use url_storage::SqliteLinkStore;

const BASE_URL: &str = "http://short.test";

async fn creator() -> (
    LinkCreator<SqliteLinkStore, RandomShortCodeGenerator>,
    SqliteLinkStore,
) {
    let store = SqliteLinkStore::open_in_memory()
        .await
        .expect("in-memory DB");
    let creator = LinkCreator::new(store.clone(), RandomShortCodeGenerator, BASE_URL);
    (creator, store)
}

/// 단축 URL에서 `BASE_URL/` 뒤의 단축 코드를 꺼낸다.
fn short_code_of(short_url: &str) -> ShortCode {
    let code = short_url
        .strip_prefix("http://short.test/")
        .unwrap_or_else(|| panic!("short URL {short_url:?} should start with BASE_URL/"));
    ShortCode::new(code)
}

#[tokio::test]
async fn creating_a_link_returns_a_short_url_whose_short_code_leads_to_the_original_url() {
    let (creator, store) = creator().await;

    let created = creator
        .create_link(OriginalUrl::new("https://example.com/some/long/path?q=1"))
        .await
        .expect("link creation succeeds");

    let short_code = short_code_of(created.short_url.as_str());
    assert_eq!(short_code.as_str().len(), 7);
    assert!(
        short_code
            .as_str()
            .chars()
            .all(|c| c.is_ascii_alphanumeric())
    );
    assert_eq!(
        created.original_url.as_str(),
        "https://example.com/some/long/path?q=1"
    );

    let stored = store
        .find_link(&short_code)
        .await
        .expect("lookup succeeds")
        .expect("link is stored under its short code");
    assert_eq!(
        stored.original_url.as_str(),
        "https://example.com/some/long/path?q=1"
    );
}

#[tokio::test]
async fn creating_twice_from_the_same_original_url_makes_two_distinct_links() {
    let (creator, store) = creator().await;
    let original_url = OriginalUrl::new("https://example.com/same");

    let first = creator.create_link(original_url.clone()).await.unwrap();
    let second = creator.create_link(original_url).await.unwrap();

    assert_ne!(first.short_url, second.short_url);
    for created in [first, second] {
        let stored = store
            .find_link(&short_code_of(created.short_url.as_str()))
            .await
            .unwrap()
            .expect("each link is stored");
        assert_eq!(stored.original_url.as_str(), "https://example.com/same");
    }
}

#[tokio::test]
async fn short_url_does_not_double_the_slash_when_base_url_ends_with_one() {
    let store = SqliteLinkStore::open_in_memory().await.unwrap();
    let creator = LinkCreator::new(store, RandomShortCodeGenerator, "http://short.test/");

    let created = creator
        .create_link(OriginalUrl::new("https://example.com"))
        .await
        .unwrap();

    assert_eq!(short_code_of(created.short_url.as_str()).as_str().len(), 7);
}

#[tokio::test]
async fn links_survive_reopening_the_database_file() {
    let dir = std::env::temp_dir().join(format!("url-storage-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // 상위 디렉터리가 없어도 열 수 있어야 한다.
    let db_path = dir.join("nested").join("links.db");

    let store = SqliteLinkStore::open(&db_path).await.expect("open file DB");
    let created = LinkCreator::new(store.clone(), RandomShortCodeGenerator, BASE_URL)
        .create_link(OriginalUrl::new("https://example.com/persisted"))
        .await
        .unwrap();
    store.close().await;

    let reopened = SqliteLinkStore::open(&db_path)
        .await
        .expect("reopen file DB");
    let stored = reopened
        .find_link(&short_code_of(created.short_url.as_str()))
        .await
        .unwrap()
        .expect("link is still stored after reopening");
    assert_eq!(
        stored.original_url.as_str(),
        "https://example.com/persisted"
    );

    reopened.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// 미리 정한 순서대로 단축 코드를 내는 생성기. 같은 코드를 일부러 반복해 충돌을 만든다.
struct ScriptedGenerator(Mutex<VecDeque<&'static str>>);

impl ScriptedGenerator {
    fn new(codes: impl IntoIterator<Item = &'static str>) -> Self {
        Self(Mutex::new(codes.into_iter().collect()))
    }
}

impl ShortCodeGenerator for ScriptedGenerator {
    fn generate(&self) -> ShortCode {
        let code = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("generator script ran out");
        ShortCode::new(code)
    }
}

#[tokio::test]
async fn short_code_collision_retries_with_a_new_short_code_and_keeps_the_existing_link() {
    let store = SqliteLinkStore::open_in_memory().await.unwrap();
    let generator = ScriptedGenerator::new(["dupCode", "dupCode", "newCode"]);
    let creator = LinkCreator::new(store.clone(), generator, BASE_URL);

    let first = creator
        .create_link(OriginalUrl::new("https://example.com/first"))
        .await
        .unwrap();
    let second = creator
        .create_link(OriginalUrl::new("https://example.com/second"))
        .await
        .expect("collision is retried, not reported");

    assert_eq!(first.short_url.as_str(), "http://short.test/dupCode");
    assert_eq!(second.short_url.as_str(), "http://short.test/newCode");

    let existing = store
        .find_link(&ShortCode::new("dupCode"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(existing.original_url.as_str(), "https://example.com/first");
    let retried = store
        .find_link(&ShortCode::new("newCode"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retried.original_url.as_str(), "https://example.com/second");
}

#[tokio::test]
async fn link_creation_fails_with_a_distinct_error_after_five_colliding_short_codes() {
    let store = SqliteLinkStore::open_in_memory().await.unwrap();
    // 첫 링크가 "dupCode"를 차지한 뒤, 두 번째 생성은 5번 연속 같은 코드를 받는다.
    // 6번째 코드("unused")가 쓰이면 상한이 지켜지지 않은 것이다.
    let generator = ScriptedGenerator::new(["dupCode"; 6].into_iter().chain(["unused"]));
    let creator = LinkCreator::new(store.clone(), generator, BASE_URL);
    creator
        .create_link(OriginalUrl::new("https://example.com/first"))
        .await
        .unwrap();

    let result = creator
        .create_link(OriginalUrl::new("https://example.com/second"))
        .await;

    assert!(
        matches!(
            result,
            Err(LinkCreationError::ShortCodeRetriesExhausted { attempts: 5 })
        ),
        "expected retries-exhausted error, got {result:?}"
    );
    let existing = store
        .find_link(&ShortCode::new("dupCode"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(existing.original_url.as_str(), "https://example.com/first");
    assert_eq!(
        store.find_link(&ShortCode::new("unused")).await.unwrap(),
        None
    );
}
