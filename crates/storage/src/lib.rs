//! 저장소 구현: 링크를 SQLite(`sqlx`)에 저장하고 불러온다.

use std::path::Path;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use url_core::{Link, LinkStore, LinkStoreError, OriginalUrl, SaveLinkError, ShortCode};

/// 저장소를 열 때의 오류.
pub use sqlx::Error as StorageError;

/// SQLite 링크 저장소. 복제해도 같은 연결 풀을 공유한다.
#[derive(Clone, Debug)]
pub struct SqliteLinkStore {
    pool: SqlitePool,
}

impl SqliteLinkStore {
    /// 파일 DB를 연다. 파일과 상위 디렉터리가 없으면 만든다.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, sqlx::Error> {
        let path = path.as_ref();
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let options = SqliteConnectOptions::new().filename(path).create_if_missing(true);
        let pool = SqlitePoolOptions::new().connect_with(options).await?;
        Self::migrated(pool).await
    }

    /// 비어 있는 in-memory DB를 연다(테스트용). 연결마다 별개 DB가 되므로 연결을 하나로 고정한다.
    pub async fn open_in_memory() -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .min_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect("sqlite::memory:")
            .await?;
        Self::migrated(pool).await
    }

    async fn migrated(pool: SqlitePool) -> Result<Self, sqlx::Error> {
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    /// 연결 풀을 닫는다. 같은 파일을 다시 열기 전에 쓴다.
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

impl LinkStore for SqliteLinkStore {
    async fn save_link(&self, link: &Link) -> Result<(), SaveLinkError> {
        // 미리 조회하지 않고 바로 INSERT한다. 충돌은 `short_code` PRIMARY KEY 제약으로 판단하므로
        // 경쟁 상태가 없고, 기존 행은 바뀌지 않는다.
        let result = sqlx::query("INSERT INTO links (short_code, original_url) VALUES (?, ?)")
            .bind(link.short_code.as_str())
            .bind(link.original_url.as_str())
            .execute(&self.pool)
            .await;
        match result {
            Ok(_) => Ok(()),
            Err(sqlx::Error::Database(db)) if db.is_unique_violation() => {
                Err(SaveLinkError::ShortCodeTaken)
            }
            Err(e) => Err(SaveLinkError::Store(LinkStoreError::new(e))),
        }
    }

    async fn find_link(&self, short_code: &ShortCode) -> Result<Option<Link>, LinkStoreError> {
        let original_url: Option<String> =
            sqlx::query_scalar("SELECT original_url FROM links WHERE short_code = ?")
                .bind(short_code.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(LinkStoreError::new)?;
        Ok(original_url.map(|url| Link {
            short_code: short_code.clone(),
            original_url: OriginalUrl::new(url),
        }))
    }
}
