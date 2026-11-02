//! 서버 전용 조립: 환경 변수를 읽어 링크 생성 서비스를 만든다.

use std::sync::Arc;

use dioxus::server::axum::Extension;
use url_core::{LinkCreator, RandomShortCodeGenerator};
use url_storage::SqliteLinkStore;

pub type LinkService = LinkCreator<SqliteLinkStore, RandomShortCodeGenerator>;

/// server function이 꺼내 쓰는 링크 생성 서비스.
pub type Creator = Extension<Arc<LinkService>>;

const DEFAULT_BASE_URL: &str = "http://localhost:8080";
const DEFAULT_DATABASE_PATH: &str = "data/links.db";

/// `BASE_URL`(기본 `http://localhost:8080`)과 `DATABASE_PATH`(기본 `data/links.db`)로 서비스를 만든다.
pub async fn creator_from_env() -> Result<Arc<LinkService>, url_storage::StorageError> {
    let base_url = std::env::var("BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.into());
    let database_path =
        std::env::var("DATABASE_PATH").unwrap_or_else(|_| DEFAULT_DATABASE_PATH.into());
    let store = SqliteLinkStore::open(&database_path).await?;
    Ok(Arc::new(LinkCreator::new(store, RandomShortCodeGenerator, base_url)))
}
