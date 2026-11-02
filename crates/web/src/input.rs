/// 입력된 원본 URL로 "생성"을 누를 수 있는지. 비어 있거나 공백뿐이면 누를 수 없다.
pub fn can_create(original_url: &str) -> bool {
    !original_url.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::can_create;

    #[test]
    fn blank_or_whitespace_only_input_cannot_create() {
        assert!(!can_create(""));
        assert!(!can_create("   "));
        assert!(!can_create(" \t\n "));
    }

    #[test]
    fn non_blank_input_can_create() {
        assert!(can_create("https://example.com"));
        assert!(can_create("  a  "));
    }
}
