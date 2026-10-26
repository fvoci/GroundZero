//! 연결의 마지막 유효한 권한 확인에 기한을 둔다. 방 작업과 소켓 송신이 기다려도 기한은 연장되지 않는다.

use std::future::Future;
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::Instant;

use super::room::{CloseReason, PeerAccess};

#[derive(Debug, Clone, Copy)]
struct State {
    checked_at: Option<Instant>,
    deadline: Instant,
    can_write: bool,
    closed: Option<CloseReason>,
}

/// 소켓과 방이 함께 보는 연결별 권한 유효 기간. 닫힌 연결은 다시 유효해지지 않는다.
#[derive(Debug, Clone)]
pub(crate) struct Lease {
    state: watch::Sender<State>,
    period: Duration,
}

impl Lease {
    pub(crate) fn new(can_write: bool, started: Instant, period: Duration) -> Self {
        let (state, _) = watch::channel(State {
            checked_at: None,
            deadline: started + period,
            can_write,
            closed: None,
        });
        Self { state, period }
    }

    fn current_at(&self, now: Instant) -> State {
        self.state.send_if_modified(|state| {
            if state.closed.is_none() && now >= state.deadline {
                state.closed = Some(CloseReason::Busy);
                true
            } else {
                false
            }
        });
        *self.state.borrow()
    }

    pub(crate) fn permission(&self) -> Option<bool> {
        let state = self.current_at(Instant::now());
        (state.closed.is_none() && state.checked_at.is_some()).then_some(state.can_write)
    }

    pub(crate) fn reason(&self) -> Option<CloseReason> {
        self.current_at(Instant::now()).closed
    }

    pub(crate) fn close(&self, reason: CloseReason) {
        self.state.send_if_modified(|state| {
            if state.closed.is_some() {
                return false;
            }
            state.closed = Some(reason);
            true
        });
    }

    /// 검사를 끝낸 때가 아닌 시작한 때를 기준으로 한다. 늦게 온 성공으로 만료된 연결을 되살리지 않는다.
    pub(crate) fn confirm(&self, started: Instant, can_write: bool) {
        self.confirm_at(started, can_write, Instant::now());
    }

    fn confirm_at(&self, started: Instant, can_write: bool, now: Instant) {
        self.state.send_if_modified(|state| {
            if state.closed.is_some() {
                return false;
            }
            if now >= state.deadline || now >= started + self.period {
                state.closed = Some(CloseReason::Busy);
            } else if state.can_write && !can_write {
                state.closed = Some(CloseReason::AccessLost);
            } else if state.checked_at.is_none_or(|last| started >= last) {
                state.checked_at = Some(started);
                state.deadline = started + self.period;
                state.can_write = can_write;
            } else {
                return false;
            }
            true
        });
    }

    pub(crate) async fn ready(&self) {
        let mut changes = self.state.subscribe();
        loop {
            let state = self.current_at(Instant::now());
            if state.closed.is_some() || state.checked_at.is_some() {
                return;
            }
            tokio::select! {
                _ = tokio::time::sleep_until(state.deadline) => {},
                _ = changes.changed() => {},
            }
        }
    }

    pub(crate) async fn closed(&self) -> CloseReason {
        let mut changes = self.state.subscribe();
        loop {
            let state = self.current_at(Instant::now());
            if let Some(reason) = state.closed {
                return reason;
            }
            tokio::select! {
                _ = tokio::time::sleep_until(state.deadline) => {},
                _ = changes.changed() => {},
            }
        }
    }

    /// 첫 검사는 즉시, 다음 검사는 유효 기간 중간에 시작해 DB 응답을 기다릴 시간을 남긴다.
    pub(crate) async fn next_check(&self) {
        let state = self.current_at(Instant::now());
        if let Some(checked_at) = state.checked_at {
            tokio::time::sleep_until(checked_at + self.period / 2).await;
        }
    }

    pub(crate) async fn monitor<F, C>(&self, mut check: C)
    where
        F: Future<Output = PeerAccess>,
        C: FnMut() -> F,
    {
        let checking = async {
            loop {
                self.next_check().await;
                let started = Instant::now();
                match check().await {
                    PeerAccess::Allowed { can_write } => self.confirm(started, can_write),
                    PeerAccess::Denied => self.close(CloseReason::AccessLost),
                    PeerAccess::RealtimeOff => self.close(CloseReason::RealtimeOff),
                    PeerAccess::Unknown => self.close(CloseReason::Busy),
                }
                if self.reason().is_some() {
                    break;
                }
            }
        };
        let _ended = self.until_closed(checking).await;
    }

    /// DB·방 참여·송신 큐·소켓 중 어디서 기다리든 기한이 먼저면 해당 작업을 버린다.
    pub(crate) async fn until_closed<T>(
        &self,
        work: impl Future<Output = T>,
    ) -> Result<T, CloseReason> {
        tokio::select! {
            biased;
            reason = self.closed() => Err(reason),
            result = work => match self.reason() {
                Some(reason) => Err(reason),
                None => Ok(result),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_checks_use_the_start_time_and_cannot_revive_an_expired_lease() {
        let start = Instant::now();
        let period = Duration::from_secs(1);
        let lease = Lease::new(true, start, period);
        lease.confirm_at(start, true, start + period / 4);
        let check = start + period / 2;
        lease.confirm_at(check, true, start + period * 3 / 4);
        assert_eq!(lease.current_at(start + period).closed, None);
        assert_eq!(
            lease.current_at(check + period).closed,
            Some(CloseReason::Busy)
        );
        lease.confirm_at(check + period, true, check + period);
        assert_eq!(
            lease.current_at(check + period).closed,
            Some(CloseReason::Busy)
        );
    }

    #[test]
    fn late_first_check_and_write_demotion_close_the_connection() {
        let start = Instant::now();
        let period = Duration::from_secs(1);
        let late = Lease::new(true, start, period);
        late.confirm_at(start, true, start + period);
        assert_eq!(late.reason(), Some(CloseReason::Busy));
        let demoted = Lease::new(true, start, period);
        demoted.confirm_at(start, false, start);
        assert_eq!(demoted.reason(), Some(CloseReason::AccessLost));
    }

    #[tokio::test]
    async fn a_closed_connection_cancels_blocked_work_without_waiting_for_it() {
        let lease = Lease::new(true, Instant::now(), Duration::from_secs(1));
        let blocked = std::future::pending::<()>();
        let close = async { lease.close(CloseReason::Busy) };
        let (outcome, ()) = tokio::join!(lease.until_closed(blocked), close);
        assert_eq!(outcome, Err(CloseReason::Busy));
    }

    #[tokio::test]
    async fn failed_checks_close_instead_of_retaining_the_last_permission() {
        for (result, expected) in [
            (PeerAccess::Unknown, CloseReason::Busy),
            (PeerAccess::Denied, CloseReason::AccessLost),
            (PeerAccess::RealtimeOff, CloseReason::RealtimeOff),
        ] {
            let lease = Lease::new(true, Instant::now(), Duration::from_secs(1));
            tokio::time::timeout(
                Duration::from_secs(1),
                lease.monitor(|| std::future::ready(result)),
            )
            .await
            .expect("a terminal check result must stop its monitor");
            assert_eq!(lease.permission(), None);
            assert_eq!(lease.reason(), Some(expected));
        }
    }

    #[tokio::test]
    async fn one_blocked_check_does_not_delay_another_connection() {
        let start = Instant::now();
        let slow = Lease::new(true, start, Duration::from_secs(1));
        let other = Lease::new(true, start, Duration::from_secs(1));
        let blocked = slow.monitor(std::future::pending::<PeerAccess>);
        let denied = async {
            other
                .monitor(|| std::future::ready(PeerAccess::Denied))
                .await;
            assert_eq!(other.reason(), Some(CloseReason::AccessLost));
            // 기한 시각을 주입해 실제 타이머/스케줄링 오차 없이 만료 경계를 검사한다.
            slow.current_at(start + Duration::from_secs(1));
        };
        tokio::join!(blocked, denied);
        assert_eq!(slow.reason(), Some(CloseReason::Busy));
    }
}
