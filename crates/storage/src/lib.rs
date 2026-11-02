//! 저장소 구현: 링크를 PostgreSQL(`sqlx`)에 저장하고 조회한다.
//!
//! 이 모듈에는 `INSERT`와 `SELECT`만 있다. `UPDATE`/`DELETE`는 코드에도 없고, 마이그레이션의 트리거가
//! DB 수준에서도 거부한다.

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use url_core::{LinkNumber, LinkStore, OriginalUrl, StoreError};

pub use sqlx::Error as StorageError;

/// PostgreSQL 링크 저장소. 복제해도 같은 연결 풀을 공유한다.
#[derive(Clone, Debug)]
pub struct PgLinkStore {
    pool: PgPool,
}

impl PgLinkStore {
    /// `DATABASE_URL` 형식의 주소로 접속하고 마이그레이션을 적용한다.
    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let store = Self::connect_lazy(database_url)?;
        store.migrate().await?;
        Ok(store)
    }

    /// 주소만 해석하고, 실제 접속은 첫 쿼리 때 맺는다. 마이그레이션은 [`Self::migrate`]로 따로 적용한다.
    pub fn connect_lazy(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect_lazy(database_url)?;
        Ok(Self { pool })
    }

    /// 마이그레이션을 적용한다. 여러 인스턴스가 동시에 불러도 sqlx가 잠금으로 직렬화한다.
    pub async fn migrate(&self) -> Result<(), sqlx::Error> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

impl LinkStore for PgLinkStore {
    async fn insert(&self, original_url: &OriginalUrl) -> Result<LinkNumber, StoreError> {
        // 번호는 IDENTITY 시퀀스가 INSERT 안에서 원자적으로 발급한다(미리 조회하지 않음).
        let id: i64 = sqlx::query_scalar("INSERT INTO links (original_url) VALUES ($1) RETURNING id")
            .bind(original_url.as_str())
            .fetch_one(&self.pool)
            .await
            .map_err(StoreError::new)?;
        LinkNumber::new(id).ok_or_else(|| StoreError::new(format!("invalid link id {id}")))
    }

    async fn find(&self, number: LinkNumber) -> Result<Option<OriginalUrl>, StoreError> {
        let url: Option<String> = sqlx::query_scalar("SELECT original_url FROM links WHERE id = $1")
            .bind(number.get())
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::new)?;
        Ok(url.map(OriginalUrl::from_stored))
    }
}
