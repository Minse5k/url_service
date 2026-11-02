//! 도메인 로직(링크 생성 등). Dioxus에 의존하지 않는다.
//! 용어는 저장소 루트의 `GLOSSARY.md`를 따른다.

use std::error::Error;
use std::fmt;
use std::future::Future;

/// 원본 URL: 사용자가 단축하려고 입력한 원래 주소. 검증하지 않는다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginalUrl(String);

impl OriginalUrl {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 단축 코드: 하나의 링크를 식별하는 짧은 문자열.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ShortCode(String);

impl ShortCode {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 단축 URL: 서비스 주소(`BASE_URL`)와 단축 코드를 합친 전체 주소.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortUrl(String);

impl ShortUrl {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 링크: 하나의 원본 URL과 하나의 단축 코드를 짝지은 기록.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub short_code: ShortCode,
    pub original_url: OriginalUrl,
}

/// 저장소가 돌려주는 오류. 구현(SQLite 등)의 오류를 감싼다.
#[derive(Debug)]
pub struct LinkStoreError(Box<dyn Error + Send + Sync>);

impl LinkStoreError {
    pub fn new(source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self(source.into())
    }
}

impl fmt::Display for LinkStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "link store error: {}", self.0)
    }
}

impl Error for LinkStoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.as_ref())
    }
}

/// 링크를 저장하지 못한 이유.
#[derive(Debug)]
pub enum SaveLinkError {
    /// 같은 단축 코드의 링크가 이미 있다(저장소의 유일성 제약으로 판단). 기존 링크는 그대로다.
    ShortCodeTaken,
    Store(LinkStoreError),
}

impl fmt::Display for SaveLinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShortCodeTaken => write!(f, "short code is already taken"),
            Self::Store(e) => e.fmt(f),
        }
    }
}

impl Error for SaveLinkError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ShortCodeTaken => None,
            Self::Store(e) => Some(e),
        }
    }
}

/// 링크 저장소.
pub trait LinkStore: Send + Sync {
    /// 새 링크를 저장한다. 단축 코드가 이미 쓰이고 있으면 덮어쓰지 않고
    /// [`SaveLinkError::ShortCodeTaken`]을 돌려준다.
    fn save_link(&self, link: &Link) -> impl Future<Output = Result<(), SaveLinkError>> + Send;

    fn find_link(
        &self,
        short_code: &ShortCode,
    ) -> impl Future<Output = Result<Option<Link>, LinkStoreError>> + Send;
}

/// 단축 코드 생성기. 링크 생성 서비스에 주입한다.
pub trait ShortCodeGenerator: Send + Sync {
    fn generate(&self) -> ShortCode;
}

/// 무작위 base62 7자 단축 코드를 만든다.
#[derive(Clone, Copy, Debug, Default)]
pub struct RandomShortCodeGenerator;

impl RandomShortCodeGenerator {
    pub const LENGTH: usize = 7;
}

const BASE62: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

impl ShortCodeGenerator for RandomShortCodeGenerator {
    fn generate(&self) -> ShortCode {
        use rand::RngExt;
        let mut rng = rand::rng();
        let code = (0..Self::LENGTH)
            .map(|_| BASE62[rng.random_range(0..BASE62.len())] as char)
            .collect::<String>();
        ShortCode(code)
    }
}

/// 링크 생성 결과.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatedLink {
    pub short_url: ShortUrl,
    pub original_url: OriginalUrl,
}

/// 링크 생성이 실패한 이유.
#[derive(Debug)]
pub enum LinkCreationError {
    Store(LinkStoreError),
    /// 새로 만든 단축 코드가 매번 이미 쓰이고 있어 재시도 상한(`attempts`회)을 넘었다.
    ShortCodeRetriesExhausted {
        attempts: usize,
    },
}

impl fmt::Display for LinkCreationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(e) => write!(f, "could not save link: {e}"),
            Self::ShortCodeRetriesExhausted { attempts } => {
                write!(
                    f,
                    "could not find a free short code after {attempts} attempts"
                )
            }
        }
    }
}

impl Error for LinkCreationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Store(e) => Some(e),
            Self::ShortCodeRetriesExhausted { .. } => None,
        }
    }
}

/// 링크 생성 서비스: 원본 URL을 받아 새 링크를 저장하고 단축 URL을 돌려준다.
pub struct LinkCreator<S, G> {
    store: S,
    generator: G,
    base_url: String,
}

impl<S: LinkStore, G: ShortCodeGenerator> LinkCreator<S, G> {
    /// 단축 코드 충돌 시 새 코드로 시도하는 최대 횟수(첫 시도 포함).
    pub const MAX_ATTEMPTS: usize = 5;

    /// `base_url`은 단축 URL 앞부분(예: `http://localhost:8080`). 끝의 `/`는 무시한다.
    pub fn new(store: S, generator: G, base_url: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_owned();
        Self {
            store,
            generator,
            base_url,
        }
    }

    pub async fn create_link(
        &self,
        original_url: OriginalUrl,
    ) -> Result<CreatedLink, LinkCreationError> {
        for _ in 0..Self::MAX_ATTEMPTS {
            let link = Link {
                short_code: self.generator.generate(),
                original_url: original_url.clone(),
            };
            match self.store.save_link(&link).await {
                Ok(()) => {
                    return Ok(CreatedLink {
                        short_url: ShortUrl(format!(
                            "{}/{}",
                            self.base_url,
                            link.short_code.as_str()
                        )),
                        original_url: link.original_url,
                    });
                }
                Err(SaveLinkError::ShortCodeTaken) => continue,
                Err(SaveLinkError::Store(e)) => return Err(LinkCreationError::Store(e)),
            }
        }
        Err(LinkCreationError::ShortCodeRetriesExhausted {
            attempts: Self::MAX_ATTEMPTS,
        })
    }
}
