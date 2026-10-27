//! 비밀 값. 로그·`Debug` 출력에 원문을 내지 않는다.

/// 로그·`Debug` 출력에서 값을 가리는 문자열.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// 값을 감싼다(테스트·명령 구성용).
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// 원문. 연결·요청에만 쓴다.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}
