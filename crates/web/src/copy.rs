use std::time::Duration;

/// 복사 결과("복사됨"/"복사 실패")를 보여 주는 시간.
pub const SHOW_FOR: Duration = Duration::from_secs(2);

/// "복사" 버튼의 라벨 상태.
///
/// 누를 때마다 새 타이머 표를 받고, `SHOW_FOR` 뒤에 그 표로 `expire`한다.
/// 그 사이 다시 눌렀다면 이전 표는 낡은 표가 되어 라벨을 일찍 되돌리지 않는다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CopyLabel {
    shown: Option<(Outcome, Timer)>,
    issued: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Copied,
    Failed,
}

/// `copied`/`failed`가 돌려주는 타이머 표.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timer(u64);

impl CopyLabel {
    pub fn text(&self) -> &'static str {
        match self.shown {
            None => "복사",
            Some((Outcome::Copied, _)) => "복사됨",
            Some((Outcome::Failed, _)) => "복사 실패",
        }
    }

    pub fn copied(&mut self) -> Timer {
        self.show(Outcome::Copied)
    }

    pub fn failed(&mut self) -> Timer {
        self.show(Outcome::Failed)
    }

    /// 타이머가 끝났다. 가장 최근 표일 때만 "복사"로 되돌린다.
    pub fn expire(&mut self, timer: Timer) {
        if self.shown.is_some_and(|(_, current)| current == timer) {
            self.shown = None;
        }
    }

    fn show(&mut self, outcome: Outcome) -> Timer {
        self.issued += 1;
        let timer = Timer(self.issued);
        self.shown = Some((outcome, timer));
        timer
    }
}

#[cfg(test)]
mod tests {
    use super::CopyLabel;

    #[test]
    fn shows_result_until_its_timer_expires() {
        let mut label = CopyLabel::default();
        assert_eq!(label.text(), "복사");
        let timer = label.copied();
        assert_eq!(label.text(), "복사됨");
        label.expire(timer);
        assert_eq!(label.text(), "복사");

        let timer = label.failed();
        assert_eq!(label.text(), "복사 실패");
        label.expire(timer);
        assert_eq!(label.text(), "복사");
    }

    #[test]
    fn a_stale_timer_does_not_reset_a_newer_click() {
        let mut label = CopyLabel::default();
        let first = label.copied();
        let second = label.copied();
        label.expire(first);
        assert_eq!(label.text(), "복사됨");
        label.expire(second);
        assert_eq!(label.text(), "복사");
    }
}
