//! 실제 PostgreSQL에 붙여 링크 번호 발급, 동시 생성, 불변성을 검증한다.
//!
//! `TEST_DATABASE_URL`이 없으면 건너뛴다. 로컬에서는:
//! `docker run -d --name url-pg -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=url_service -p 55432:5432 postgres:17-alpine`
//! `TEST_DATABASE_URL=postgres://postgres:postgres@localhost:55432/url_service cargo test -p url-storage`

use std::collections::HashSet;

use url_core::{LinkNumber, LinkStore, OriginalUrl, create_link};
use url_storage::PgLinkStore;

async fn store() -> Option<PgLinkStore> {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL not set; skipping Postgres test");
        return None;
    };
    Some(PgLinkStore::connect(&url).await.expect("connect to test database"))
}

fn url(s: &str) -> OriginalUrl {
    OriginalUrl::parse(s, None).unwrap()
}

#[tokio::test]
async fn created_link_is_found_by_its_number_with_path_query_fragment() {
    let Some(store) = store().await else { return };
    let created = create_link(&store, "https://s.example", "https://example.org/a?b=1#c").await.unwrap();

    assert_eq!(created.short_url, format!("https://s.example/{}", created.number));
    let found = store.find(created.number).await.unwrap().unwrap();
    assert_eq!(found.as_str(), "https://example.org/a?b=1#c");
}

#[tokio::test]
async fn same_original_url_gets_a_new_number_each_time() {
    let Some(store) = store().await else { return };
    let a = store.insert(&url("https://example.org/same")).await.unwrap();
    let b = store.insert(&url("https://example.org/same")).await.unwrap();
    assert_ne!(a, b);
}

#[tokio::test]
async fn unknown_number_is_not_found() {
    let Some(store) = store().await else { return };
    assert!(store.find(LinkNumber::new(i64::MAX).unwrap()).await.unwrap().is_none());
}

#[tokio::test]
async fn concurrent_creations_get_distinct_numbers_and_keep_their_urls() {
    let Some(store) = store().await else { return };
    let tasks: Vec<_> = (0..100)
        .map(|i| {
            let store = store.clone();
            tokio::spawn(async move {
                let original = url(&format!("https://example.org/concurrent/{i}"));
                (store.insert(&original).await.unwrap(), original)
            })
        })
        .collect();

    let mut numbers = HashSet::new();
    for task in tasks {
        let (number, original) = task.await.unwrap();
        assert!(numbers.insert(number), "number {number} issued twice");
        assert_eq!(store.find(number).await.unwrap().unwrap(), original);
    }
    assert_eq!(numbers.len(), 100);
}

#[tokio::test]
async fn database_rejects_changing_or_deleting_a_link() {
    let Some(store) = store().await else { return };
    let number = store.insert(&url("https://example.org/immutable")).await.unwrap();

    let update = sqlx::query("UPDATE links SET original_url = 'https://evil.example' WHERE id = $1")
        .bind(number.get())
        .execute(store.pool())
        .await;
    assert!(update.is_err(), "UPDATE must be rejected");

    let delete = sqlx::query("DELETE FROM links WHERE id = $1").bind(number.get()).execute(store.pool()).await;
    assert!(delete.is_err(), "DELETE must be rejected");

    let explicit_id = sqlx::query("INSERT INTO links (id, original_url) VALUES ($1, 'https://evil.example')")
        .bind(number.get())
        .execute(store.pool())
        .await;
    assert!(explicit_id.is_err(), "choosing a number explicitly must be rejected");

    let found = store.find(number).await.unwrap().unwrap();
    assert_eq!(found.as_str(), "https://example.org/immutable");
}

#[tokio::test]
async fn database_rejects_non_http_urls_even_if_code_is_bypassed() {
    let Some(store) = store().await else { return };
    let bad = sqlx::query("INSERT INTO links (original_url) VALUES ('javascript:alert(1)')")
        .execute(store.pool())
        .await;
    assert!(bad.is_err());
}
