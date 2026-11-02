//! 도메인 로직: 원본 URL 검증, 링크 번호, 링크 생성·조회.
//! Dioxus와 DB에 의존하지 않는다. 용어는 저장소 루트의 `GLOSSARY.md`를 따른다.

use std::error::Error;
use std::fmt;
use std::future::Future;

use url::Url;

/// 원본 URL의 최대 길이(바이트).
pub const MAX_ORIGINAL_URL_LEN: usize = 2048;

/// 원본 URL: 검증을 통과한 `http://` 또는 `https://` 절대 URL.
///
/// 경로·쿼리·프래그먼트는 그대로 유지한다. 저장·리디렉션에는 `url` 크레이트가 표준화한 문자열
/// (호스트 소문자화, 필요한 퍼센트 인코딩)을 쓰므로 HTTP `Location` 헤더에 안전하다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginalUrl(String);

impl OriginalUrl {
    /// 사용자 입력을 검증한다. 앞뒤 공백은 무시한다.
    ///
    /// `service_host`가 주어지면 그 호스트를 가리키는 URL(단축 URL을 다시 단축하는 루프)은 거부한다.
    pub fn parse(input: &str, service_host: Option<&str>) -> Result<Self, InvalidUrl> {
        let input = input.trim();
        if input.is_empty() {
            return Err(InvalidUrl::Empty);
        }
        if input.len() > MAX_ORIGINAL_URL_LEN {
            return Err(InvalidUrl::TooLong);
        }
        let url = Url::parse(input).map_err(|_| InvalidUrl::Malformed)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(InvalidUrl::UnsupportedScheme);
        }
        let host = url.host_str().filter(|h| !h.is_empty()).ok_or(InvalidUrl::MissingHost)?;
        if service_host.is_some_and(|own| own.eq_ignore_ascii_case(host)) {
            return Err(InvalidUrl::PointsToService);
        }
        if url.as_str().len() > MAX_ORIGINAL_URL_LEN {
            return Err(InvalidUrl::TooLong);
        }
        Ok(Self(url.into()))
    }

    /// DB에서 읽은 값(이미 검증해 저장한 값)으로 만든다.
    pub fn from_stored(value: String) -> Self {
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 원본 URL이 받아들여지지 않은 이유.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidUrl {
    Empty,
    TooLong,
    Malformed,
    UnsupportedScheme,
    MissingHost,
    PointsToService,
}

impl fmt::Display for InvalidUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "원본 URL을 입력하세요.",
            Self::TooLong => "원본 URL이 너무 깁니다(최대 2048자).",
            Self::Malformed => "올바른 URL 형식이 아닙니다.",
            Self::UnsupportedScheme => "http:// 또는 https:// 로 시작하는 URL만 사용할 수 있습니다.",
            Self::MissingHost => "URL에 호스트가 없습니다.",
            Self::PointsToService => "이 서비스의 단축 URL은 다시 단축할 수 없습니다.",
        })
    }
}

impl Error for InvalidUrl {}

/// 링크 번호: 단축 URL의 경로(`/123`)가 되는 양의 정수. DB가 발급한다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LinkNumber(i64);

impl LinkNumber {
    /// DB가 발급한 값으로 만든다.
    pub fn new(value: i64) -> Option<Self> {
        (value > 0).then_some(Self(value))
    }

    /// 요청 경로 조각(`"123"`)을 해석한다. ASCII 숫자만 허용하고 부호, 공백, 0, 앞자리 0(`"042"`)은
    /// 거부한다. 그래서 하나의 링크 번호에는 하나의 경로만 대응한다.
    pub fn parse_path(segment: &str) -> Option<Self> {
        if segment.starts_with('0') || !segment.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        segment.parse::<i64>().ok().and_then(Self::new)
    }

    pub fn get(self) -> i64 {
        self.0
    }
}

impl fmt::Display for LinkNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// 저장소 오류. 구현(Postgres 등)의 오류를 감싼다.
#[derive(Debug)]
pub struct StoreError(Box<dyn Error + Send + Sync>);

impl StoreError {
    pub fn new(source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self(source.into())
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "link store error: {}", self.0)
    }
}

impl Error for StoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.as_ref())
    }
}

/// 링크 저장소. 링크는 추가와 조회만 가능하다.
///
/// 수정·삭제 연산은 의도적으로 없다. 발급한 링크 번호와 원본 URL의 연결은 바뀌지 않는다.
pub trait LinkStore: Send + Sync {
    /// 새 링크를 저장하고, 저장소가 새로 발급한 링크 번호를 돌려준다.
    /// 번호는 저장소가 원자적으로 발급하므로 동시 요청에서도 겹치지 않고 재사용되지 않는다.
    fn insert(
        &self,
        original_url: &OriginalUrl,
    ) -> impl Future<Output = Result<LinkNumber, StoreError>> + Send;

    fn find(
        &self,
        number: LinkNumber,
    ) -> impl Future<Output = Result<Option<OriginalUrl>, StoreError>> + Send;
}

/// 단축 URL = 서비스 주소 + `/` + 링크 번호.
pub fn short_url(base_url: &str, number: LinkNumber) -> String {
    format!("{}/{}", base_url.trim_end_matches('/'), number)
}

/// 링크 생성 결과.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatedLink {
    pub number: LinkNumber,
    pub short_url: String,
    pub original_url: OriginalUrl,
}

/// 링크 생성이 실패한 이유.
#[derive(Debug)]
pub enum CreateLinkError {
    Invalid(InvalidUrl),
    Store(StoreError),
}

impl fmt::Display for CreateLinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(e) => e.fmt(f),
            Self::Store(e) => e.fmt(f),
        }
    }
}

impl Error for CreateLinkError {}

/// 원본 URL을 검증하고 새 링크를 만든다. 같은 원본 URL이라도 매번 새 링크 번호를 발급한다.
pub async fn create_link<S: LinkStore>(
    store: &S,
    base_url: &str,
    input: &str,
) -> Result<CreatedLink, CreateLinkError> {
    let service_host = Url::parse(base_url).ok().and_then(|u| u.host_str().map(str::to_owned));
    let original_url =
        OriginalUrl::parse(input, service_host.as_deref()).map_err(CreateLinkError::Invalid)?;
    let number = store.insert(&original_url).await.map_err(CreateLinkError::Store)?;
    Ok(CreatedLink { number, short_url: short_url(base_url, number), original_url })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_http_and_https_and_keeps_path_query_fragment() {
        let url = OriginalUrl::parse("https://example.org/a/b?x=1&y=2#frag", None).unwrap();
        assert_eq!(url.as_str(), "https://example.org/a/b?x=1&y=2#frag");
        assert!(OriginalUrl::parse("http://example.org", None).is_ok());
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let url = OriginalUrl::parse("  https://example.org/x \n", None).unwrap();
        assert_eq!(url.as_str(), "https://example.org/x");
    }

    #[test]
    fn rejects_non_http_urls_and_garbage() {
        assert_eq!(OriginalUrl::parse("", None), Err(InvalidUrl::Empty));
        assert_eq!(OriginalUrl::parse("   ", None), Err(InvalidUrl::Empty));
        assert_eq!(OriginalUrl::parse("hello", None), Err(InvalidUrl::Malformed));
        assert_eq!(OriginalUrl::parse("example.org", None), Err(InvalidUrl::Malformed));
        assert_eq!(OriginalUrl::parse("ftp://example.org", None), Err(InvalidUrl::UnsupportedScheme));
        assert_eq!(OriginalUrl::parse("javascript:alert(1)", None), Err(InvalidUrl::UnsupportedScheme));
        assert_eq!(OriginalUrl::parse("mailto:a@b.c", None), Err(InvalidUrl::UnsupportedScheme));
        assert!(OriginalUrl::parse("https://", None).is_err());
    }

    #[test]
    fn rejects_too_long_urls() {
        let long = format!("https://example.org/{}", "a".repeat(MAX_ORIGINAL_URL_LEN));
        assert_eq!(OriginalUrl::parse(&long, None), Err(InvalidUrl::TooLong));
    }

    #[test]
    fn rejects_urls_pointing_at_the_service_itself() {
        assert_eq!(
            OriginalUrl::parse("https://Short.Example/12", Some("short.example")),
            Err(InvalidUrl::PointsToService)
        );
    }

    #[test]
    fn link_number_path_accepts_only_positive_ascii_digits() {
        assert_eq!(LinkNumber::parse_path("123").map(LinkNumber::get), Some(123));
        for bad in ["", "0", "0042", "-1", "+1", "1a", "abc", " 1", "١٢", "99999999999999999999"] {
            assert_eq!(LinkNumber::parse_path(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn short_url_joins_base_and_number() {
        let n = LinkNumber::new(7).unwrap();
        assert_eq!(short_url("https://s.example/", n), "https://s.example/7");
        assert_eq!(short_url("https://s.example", n), "https://s.example/7");
    }

    struct OneStore;
    impl LinkStore for OneStore {
        async fn insert(&self, _: &OriginalUrl) -> Result<LinkNumber, StoreError> {
            Ok(LinkNumber::new(1).unwrap())
        }
        async fn find(&self, _: LinkNumber) -> Result<Option<OriginalUrl>, StoreError> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn create_link_validates_before_storing_and_builds_short_url() {
        let created = create_link(&OneStore, "https://s.example", " https://a.org/p?q#f ").await.unwrap();
        assert_eq!(created.short_url, "https://s.example/1");
        assert_eq!(created.original_url.as_str(), "https://a.org/p?q#f");

        let err = create_link(&OneStore, "https://s.example", "ftp://a.org").await.unwrap_err();
        assert!(matches!(err, CreateLinkError::Invalid(InvalidUrl::UnsupportedScheme)));
        let err = create_link(&OneStore, "https://s.example", "https://s.example/1").await.unwrap_err();
        assert!(matches!(err, CreateLinkError::Invalid(InvalidUrl::PointsToService)));
    }
}
