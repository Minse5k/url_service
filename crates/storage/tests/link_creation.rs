//! 링크 생성 seam 테스트: `url-core`의 링크 생성 서비스를 실제 SQLite 저장소에 붙여 호출한다.
//! 반환된 단축 URL과 저장된 링크만 본다.

use url_core::{LinkCreator, LinkStore, OriginalUrl, RandomShortCodeGenerator, ShortCode};
use url_storage::SqliteLinkStore;

const BASE_URL: &str = "http://short.test";

async fn creator() -> (LinkCreator<SqliteLinkStore, RandomShortCodeGenerator>, SqliteLinkStore) {
    let store = SqliteLinkStore::open_in_memory().await.expect("in-memory DB");
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
    assert!(short_code.as_str().chars().all(|c| c.is_ascii_alphanumeric()));
    assert_eq!(created.original_url.as_str(), "https://example.com/some/long/path?q=1");

    let stored = store
        .find_link(&short_code)
        .await
        .expect("lookup succeeds")
        .expect("link is stored under its short code");
    assert_eq!(stored.original_url.as_str(), "https://example.com/some/long/path?q=1");
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

    let created = creator.create_link(OriginalUrl::new("https://example.com")).await.unwrap();

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

    let reopened = SqliteLinkStore::open(&db_path).await.expect("reopen file DB");
    let stored = reopened
        .find_link(&short_code_of(created.short_url.as_str()))
        .await
        .unwrap()
        .expect("link is still stored after reopening");
    assert_eq!(stored.original_url.as_str(), "https://example.com/persisted");

    reopened.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}
