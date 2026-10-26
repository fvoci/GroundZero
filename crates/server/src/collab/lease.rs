//! 연결의 마지막 유효한 권한 확인에 기한을 둔다. 방 작업과 소켓 송신이 기다려도 기한은 연장되지 않는다.

use std::collections::VecDeque;
use std::future::Future;
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::Instant;

use super::room::{CloseReason, PeerAccess};

#[derive(Debug, Clone, Copy)]
struct Closed {
    reason: CloseReason,
    at: Instant,
}

#[derive(Debug, Clone, Copy)]
struct Check {
    started: Instant,
    finished: Instant,
}

#[derive(Debug, Clone, Copy)]
struct State {
    check: Option<Check>,
    deadline: Instant,
    can_write: bool,
    closed: Option<Closed>,
}

#[derive(Debug)]
struct Verifications {
    current: State,
    /// 커밋 경계가 나중에 알려져도 마지막 source 확인을 찾는다. 유효 기간이 끝난 기록은 남기지 않는다.
    checks: VecDeque<Check>,
}

/// 소켓과 방이 함께 보는 연결별 권한 유효 기간. 닫힌 연결은 다시 유효해지지 않는다.
#[derive(Debug, Clone)]
pub(crate) struct Lease {
    state: watch::Sender<Verifications>,
    period: Duration,
}

/// 수신 당시의 쓰기 근거. 이동 커밋 전 큐 편집의 대리 저장에만 쓰며 연결을 다시 열거나 기한을 연장하지 않는다.
#[derive(Debug, Clone)]
pub(crate) struct WritePermit {
    access: Lease,
    deadline: Instant,
}

impl WritePermit {
    pub(crate) fn freeze_move(&self, committed: std::time::Instant) -> Option<MovePermit> {
        self.freeze_move_at(Instant::from_std(committed), Instant::now())
    }

    #[cfg(test)]
    fn move_deadline_at(&self, committed: Instant, now: Instant) -> Option<Instant> {
        self.freeze_move_at(committed, now)
            .and_then(|permit| permit.deadline_at(now))
    }

    fn freeze_move_at(&self, committed: Instant, now: Instant) -> Option<MovePermit> {
        self.access.current_at(now);
        let source_deadline = self
            .access
            .state
            .borrow()
            .checks
            .iter()
            .filter(|check| check.finished <= committed)
            .map(|check| check.started + self.access.period)
            .max()
            .unwrap_or(self.deadline)
            .max(self.deadline);
        let permit = MovePermit {
            access: self.access.clone(),
            committed,
            deadline: source_deadline,
        };
        permit.deadline_at(now).map(|_| permit)
    }
}

/// 커밋 경계에서 확정한 대리 저장 근거. 현재 위치의 성공 확인은 이 기한을 바꾸지 않는다.
#[derive(Debug, Clone)]
pub(crate) struct MovePermit {
    access: Lease,
    committed: Instant,
    deadline: Instant,
}

impl MovePermit {
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.deadline_at(Instant::now())
    }

    fn deadline_at(&self, now: Instant) -> Option<Instant> {
        let state = self.access.current_at(now);
        if state.closed.is_some_and(|closed| {
            closed.at <= self.committed
                || !matches!(
                    closed.reason,
                    CloseReason::AccessLost | CloseReason::RealtimeOff
                )
        }) {
            return None;
        }
        (now < self.deadline).then_some(self.deadline)
    }

    pub(crate) async fn check_session(
        &self,
        check: impl Future<Output = Option<bool>>,
    ) -> Result<(), CloseReason> {
        let deadline = self.deadline().ok_or(CloseReason::Busy)?;
        match tokio::time::timeout_at(deadline, check).await {
            Ok(Some(true)) if self.deadline().is_some() => Ok(()),
            Ok(Some(false)) => Err(CloseReason::AccessLost),
            _ => Err(CloseReason::Busy),
        }
    }
}

impl Lease {
    pub(crate) fn new(can_write: bool, started: Instant, period: Duration) -> Self {
        let (state, _) = watch::channel(Verifications {
            current: State {
                check: None,
                deadline: started + period,
                can_write,
                closed: None,
            },
            checks: VecDeque::new(),
        });
        Self { state, period }
    }

    fn current_at(&self, now: Instant) -> State {
        self.state.send_if_modified(|verifications| {
            let previous_len = verifications.checks.len();
            verifications
                .checks
                .retain(|check| now < check.started + self.period);
            let state = &mut verifications.current;
            if state.closed.is_none() && now >= state.deadline {
                state.closed = Some(Closed {
                    reason: CloseReason::Busy,
                    at: state.deadline,
                });
                true
            } else {
                verifications.checks.len() != previous_len
            }
        });
        self.state.borrow().current
    }

    pub(crate) fn permission(&self) -> Option<bool> {
        let state = self.current_at(Instant::now());
        (state.closed.is_none() && state.check.is_some()).then_some(state.can_write)
    }

    pub(crate) fn write_permit(&self) -> Option<WritePermit> {
        let state = self.current_at(Instant::now());
        (state.closed.is_none() && state.check.is_some() && state.can_write).then(|| WritePermit {
            access: self.clone(),
            deadline: state.deadline,
        })
    }

    pub(crate) fn reason(&self) -> Option<CloseReason> {
        self.current_at(Instant::now())
            .closed
            .map(|closed| closed.reason)
    }

    pub(crate) fn close(&self, reason: CloseReason) {
        self.close_at(reason, Instant::now());
    }

    fn close_at(&self, reason: CloseReason, at: Instant) {
        self.state.send_if_modified(|verifications| {
            let state = &mut verifications.current;
            if state.closed.is_some() {
                return false;
            }
            state.closed = Some(Closed { reason, at });
            true
        });
    }

    /// 검사를 끝낸 때가 아닌 시작한 때를 기준으로 한다. 늦게 온 성공으로 만료된 연결을 되살리지 않는다.
    pub(crate) fn confirm(&self, started: Instant, can_write: bool) {
        self.confirm_at(started, can_write, Instant::now());
    }

    fn confirm_at(&self, started: Instant, can_write: bool, now: Instant) {
        self.state.send_if_modified(|verifications| {
            let state = &mut verifications.current;
            if state.closed.is_some() {
                return false;
            }
            if now >= state.deadline || now >= started + self.period {
                state.closed = Some(Closed {
                    reason: CloseReason::Busy,
                    at: state.deadline.min(started + self.period),
                });
            } else if state.can_write && !can_write {
                state.closed = Some(Closed {
                    reason: CloseReason::AccessLost,
                    at: now,
                });
            } else if state.check.is_none_or(|last| started >= last.started) {
                let check = Check {
                    started,
                    finished: now,
                };
                state.check = Some(check);
                state.deadline = started + self.period;
                state.can_write = can_write;
                verifications
                    .checks
                    .retain(|check| now < check.started + self.period);
                verifications.checks.push_back(check);
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
            if state.closed.is_some() || state.check.is_some() {
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
                return reason.reason;
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
        if let Some(check) = state.check {
            tokio::time::sleep_until(check.started + self.period / 2).await;
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
        assert!(lease.current_at(start + period).closed.is_none());
        assert_eq!(
            lease
                .current_at(check + period)
                .closed
                .map(|closed| closed.reason),
            Some(CloseReason::Busy)
        );
        lease.confirm_at(check + period, true, check + period);
        assert_eq!(
            lease
                .current_at(check + period)
                .closed
                .map(|closed| closed.reason),
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

    #[test]
    fn move_permits_distinguish_new_place_denial_from_old_revocation_and_unknown() {
        let start = Instant::now();
        let commit = start + Duration::from_millis(400);
        for (reason, closed_after_commit, allowed) in [
            (CloseReason::AccessLost, true, true),
            (CloseReason::RealtimeOff, true, true),
            (CloseReason::AccessLost, false, false),
            (CloseReason::RealtimeOff, false, false),
            (CloseReason::Busy, true, false),
            (CloseReason::Invalid, true, false),
        ] {
            let lease = Lease::new(true, start, Duration::from_secs(1));
            lease.confirm_at(start, true, start);
            let permit = lease.write_permit().unwrap();
            let ended = start + Duration::from_millis(if closed_after_commit { 500 } else { 300 });
            lease.close_at(reason, ended);
            assert_eq!(
                permit
                    .move_deadline_at(commit, start + Duration::from_millis(600))
                    .is_some(),
                allowed,
                "reason={reason:?}, closed_after_commit={closed_after_commit}"
            );
            assert_eq!(
                lease.permission(),
                None,
                "a move permit must never revive the connection"
            );
            assert!(
                permit
                    .move_deadline_at(commit, start + Duration::from_secs(1))
                    .is_none(),
                "the source deadline still applies: {reason:?}"
            );
        }
    }

    #[test]
    fn only_source_rechecks_can_refresh_a_queued_move_permit() {
        let start = Instant::now();
        let commit = start + Duration::from_millis(400);
        for check_before_commit in [true, false] {
            let lease = Lease::new(true, start, Duration::from_secs(1));
            lease.confirm_at(start, true, start);
            let permit = lease.write_permit().unwrap();
            let checked =
                start + Duration::from_millis(if check_before_commit { 300 } else { 500 });
            lease.confirm_at(checked, true, checked);
            let now = start + Duration::from_millis(1100);
            assert_eq!(
                permit.move_deadline_at(commit, now).is_some(),
                check_before_commit
            );
        }
        let reader = Lease::new(false, start, Duration::from_secs(1));
        reader.confirm_at(start, false, start);
        assert!(reader.write_permit().is_none());
    }

    #[test]
    fn a_check_started_before_commit_but_finished_after_it_cannot_refresh_source_proof() {
        let start = Instant::now();
        let lease = Lease::new(true, start, Duration::from_secs(1));
        lease.confirm_at(start, true, start);
        let permit = lease.write_permit().unwrap();
        let commit = start + Duration::from_millis(400);
        lease.confirm_at(
            start + Duration::from_millis(300),
            true,
            start + Duration::from_millis(500),
        );
        assert!(
            permit
                .move_deadline_at(commit, start + Duration::from_millis(1100))
                .is_none()
        );
    }

    #[tokio::test]
    async fn delegated_session_failure_and_delay_are_not_accepted() {
        for outcome in [None, Some(false)] {
            let start = Instant::now();
            let lease = Lease::new(true, start, Duration::from_secs(1));
            lease.confirm(start, true);
            let permit = lease.write_permit().unwrap();
            let committed = std::time::Instant::now();
            let permit = permit.freeze_move(committed).unwrap();
            lease.close(CloseReason::AccessLost);
            let result = permit.check_session(std::future::ready(outcome)).await;
            assert_eq!(
                result,
                Err(if outcome.is_none() {
                    CloseReason::Busy
                } else {
                    CloseReason::AccessLost
                })
            );
            assert_eq!(lease.permission(), None);
        }
        let start = Instant::now();
        let lease = Lease::new(true, start, Duration::from_secs(1));
        lease.confirm(start, true);
        let permit = lease.write_permit().unwrap();
        let committed = std::time::Instant::now();
        let permit = permit.freeze_move(committed).unwrap();
        lease.close(CloseReason::RealtimeOff);
        assert_eq!(
            permit.check_session(std::future::pending()).await,
            Err(CloseReason::Busy)
        );
    }

    #[test]
    fn destination_success_does_not_replace_the_last_completed_source_deadline() {
        for observed_before_destination in [false, true] {
            let start = Instant::now();
            let lease = Lease::new(true, start, Duration::from_secs(1));
            lease.confirm_at(start, true, start);
            let permit = lease.write_permit().unwrap();
            let source = start + Duration::from_millis(300);
            lease.confirm_at(source, true, source);
            let committed = start + Duration::from_millis(400);
            let expected = start + Duration::from_millis(1300);
            let frozen = observed_before_destination
                .then(|| permit.freeze_move_at(committed, committed).unwrap());
            for millis in [500, 600, 700, 800] {
                let destination = start + Duration::from_millis(millis);
                lease.confirm_at(destination, true, destination);
            }
            let now = start + Duration::from_millis(1100);
            let frozen = frozen.unwrap_or_else(|| permit.freeze_move_at(committed, now).unwrap());
            assert_eq!(
                frozen.deadline_at(now),
                Some(expected),
                "observed_before_destination={observed_before_destination}"
            );
            assert!(
                frozen.deadline_at(expected).is_none(),
                "destination success must not extend the frozen source deadline"
            );
        }
    }
}
