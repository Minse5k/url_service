use std::time::Duration;

/// 복사 후 "복사됨"을 보여 주는 시간.
pub const COPIED_FOR: Duration = Duration::from_secs(2);

/// 단축 URL "복사" 버튼의 라벨 상태.
///
/// 복사할 때마다 새 타이머 표를 받고, `COPIED_FOR` 뒤에 그 표로 `expire`한다.
/// 그 사이 다시 복사했다면 이전 표는 낡은 표가 되어 라벨을 되돌리지 않는다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CopyLabel {
    /// 가장 최근 복사의 타이머 표. `None`이면 "복사" 상태.
    current: Option<Timer>,
    issued: u64,
}

/// `CopyLabel::copied`가 돌려주는 타이머 표.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timer(u64);

impl CopyLabel {
    pub fn text(&self) -> &'static str {
        if self.current.is_some() { "복사됨" } else { "복사" }
    }

    /// 복사에 성공했다. "복사됨"으로 바꾸고 이번 복사의 타이머 표를 돌려준다.
    pub fn copied(&mut self) -> Timer {
        self.issued += 1;
        let timer = Timer(self.issued);
        self.current = Some(timer);
        timer
    }

    /// 타이머가 끝났다. 가장 최근 복사의 표일 때만 "복사"로 되돌린다.
    pub fn expire(&mut self, timer: Timer) {
        if self.current == Some(timer) {
            self.current = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CopyLabel;

    #[test]
    fn shows_copy_before_any_click() {
        assert_eq!(CopyLabel::default().text(), "복사");
    }

    #[test]
    fn shows_copied_until_its_timer_expires() {
        let mut label = CopyLabel::default();
        let timer = label.copied();
        assert_eq!(label.text(), "복사됨");
        label.expire(timer);
        assert_eq!(label.text(), "복사");
    }

    #[test]
    fn clicking_again_restarts_the_timer() {
        let mut label = CopyLabel::default();
        let first = label.copied();
        let second = label.copied();

        label.expire(first);
        assert_eq!(label.text(), "복사됨", "stale timer must not flip the label back");

        label.expire(second);
        assert_eq!(label.text(), "복사");
    }
}
