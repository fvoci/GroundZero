//! 공유 목록 오염의 격리와 재시작 경계(#172).

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Copy)]
pub enum Registry {
    Rooms,
    Closing,
    Held,
}

/// 격리 통합 시험에서 실제 목록 잠금을 쥔 패닉을 재현한다. 배포 빌드에는 포함하지 않는다.
#[cfg(debug_assertions)]
pub fn poison(rooms: &super::Rooms, registry: Registry) {
    let _panicked = std::panic::catch_unwind(|| match registry {
        Registry::Rooms => {
            let _guard = rooms.inner.lock();
            panic!("injected room registry panic");
        }
        Registry::Closing => {
            let _guard = rooms.closing.lock();
            panic!("injected closing registry panic");
        }
        Registry::Held => {
            let _guard = rooms.held.lock();
            panic!("injected held registry panic");
        }
    });
}
