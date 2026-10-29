//! 공유 목록 오염의 격리와 재시작 경계(#172).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use tokio::sync::watch;

use super::{CloseReason, Command, Handle, Outbound, Room, Rooms};
use fvoci_core::ids::DocumentId;

#[derive(Debug)]
pub(super) struct Epoch {
    changed: watch::Sender<u64>,
    reset_needed: AtomicBool,
}

impl Default for Epoch {
    fn default() -> Self {
        Self {
            changed: watch::channel(0).0,
            reset_needed: AtomicBool::new(false),
        }
    }
}

impl Epoch {
    pub(super) fn current(&self) -> u64 {
        *self.changed.borrow()
    }
    pub(super) fn subscribe(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }
    fn invalidate(&self) {
        self.reset_needed.store(true, Ordering::Release);
        self.changed.send_modify(|epoch| *epoch += 1);
    }
}

/// 오염된 방 목록은 이어 쓰지 않는다. 두 목록은 한 불변식을 이루므로 함께 비우고, 독립된 watch로 모든 옛 방을 닫는다.
/// 깨진 목록에서 빠진 핸들도 있을 수 있어 목록에 남은 핸들만 순회해서 닫지 않는다. 프로세스·저장 상태는 유지한다.
pub(super) fn lock(rooms: &Rooms) -> MutexGuard<'_, HashMap<DocumentId, Handle>> {
    let mut inner = rooms
        .inner
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if rooms.inner.is_poisoned()
        || rooms.held.is_poisoned()
        || rooms.recovery.reset_needed.swap(false, Ordering::AcqRel)
    {
        let mut held = rooms
            .held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.clear();
        held.clear();
        rooms.inner.clear_poison();
        rooms.held.clear_poison();
        rooms.recovery.invalidate();
        rooms.recovery.reset_needed.store(false, Ordering::Release);
        tracing::warn!(
            event = "collab.registry_reset",
            "poisoned registries discarded; rooms reconnect from stored state"
        );
    }
    inner
}

/// 보조 목록은 방 목록 아래에서만 잠근다. 별도 패닉과 엇갈려도 오염된 막음을 버리고 옛 방들을 종료시킨다.
pub(super) fn list<'a, T: Default>(mutex: &'a Mutex<T>, epoch: &Epoch) -> MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(error) => {
            let mut guard = error.into_inner();
            *guard = T::default();
            mutex.clear_poison();
            epoch.invalidate();
            guard
        }
    }
}

/// DB를 기다리는 방도 즉시 취소한다. 아직 커밋하지 않은 트랜잭션은 롤백하고 새 방이 저장된 상태를 이어받는다.
pub(super) async fn run(room: Room, rx: tokio::sync::mpsc::Receiver<Command>) {
    let mut epoch = room.registry_epoch.clone();
    let mut task = tokio::spawn(room.run(rx));
    tokio::select! {
        biased;
        _ = epoch.changed() => {
            task.abort();
            let _ended = task.await;
        }
        _ = &mut task => {}
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        for connection in self.conns.values() {
            connection.peer.access.close(CloseReason::Busy);
            let _sent = connection
                .peer
                .out
                .try_send(Outbound::Close(CloseReason::Busy));
        }
    }
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Copy)]
pub enum Registry {
    Rooms,
    Held,
}

/// 격리 통합 시험에서 실제 목록 잠금을 쥔 패닉을 재현한다. 배포 빌드에는 포함하지 않는다.
#[cfg(debug_assertions)]
pub fn poison(rooms: &super::Rooms, registry: Registry) {
    let _panicked = std::panic::catch_unwind(|| match registry {
        Registry::Rooms => {
            let mut guard = rooms
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.clear(); // 목록에서 빠진 방도 독립된 epoch로 종료해야 한다.
            std::panic::resume_unwind(Box::new("injected room registry panic"));
        }
        Registry::Held => {
            let _guard = rooms.held.lock();
            std::panic::resume_unwind(Box::new("injected held registry panic"));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn an_old_hold_guard_cannot_release_a_new_barrier_after_recovery() {
        let rooms = Rooms::default();
        let doc = DocumentId(Uuid::now_v7());
        let (old, _) = rooms.hold(doc);
        poison(&rooms, Registry::Held);
        let (new, _) = rooms.hold(doc);
        drop(old);
        {
            let _rooms = rooms.lock();
            assert_eq!(rooms.held_set().get(&doc), Some(&1));
        }
        drop(new);
        let _rooms = rooms.lock();
        assert!(rooms.held_set().is_empty());
    }
}
