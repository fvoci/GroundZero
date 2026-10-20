//! 제품 버전과 소스 식별자(D-87). 릴리스 이미지는 검증한 Git 태그의 버전(`v0.0.1` → `0.0.1`)과 그 태그 커밋의 전체 SHA를
//! 빌드할 때 환경 변수 `FVOCI_RELEASE_VERSION`·`FVOCI_SOURCE_SHA`로 넣는다(`deploy/docker/app.Dockerfile`의 build arg).
//! 둘 다 없으면 개발 빌드로 둘 다 `dev`다. 하나만 있거나 형식이 틀리면 컴파일이 멈춘다. Cargo 패키지 버전
//! (`CARGO_PKG_VERSION`)은 제품 버전이 아니므로 여기서 쓰지 않는다.

use serde::Serialize;
use utoipa::ToSchema;

const RELEASE_VERSION: &str = match option_env!("FVOCI_RELEASE_VERSION") {
    Some(v) => v,
    None => "",
};
const SOURCE_SHA: &str = match option_env!("FVOCI_SOURCE_SHA") {
    Some(v) => v,
    None => "",
};

/// 개발 빌드 표시. 정식 릴리스 버전과 헷갈리지 않는 값이다.
pub const DEV: &str = "dev";

/// 제품 버전(`MAJOR.MINOR.PATCH`) 또는 `dev`.
pub const VERSION: &str = if RELEASE_VERSION.is_empty() {
    DEV
} else {
    RELEASE_VERSION
};

/// 빌드한 소스 커밋의 전체 SHA(소문자 40자리) 또는 `dev`.
pub const SOURCE: &str = if SOURCE_SHA.is_empty() {
    DEV
} else {
    SOURCE_SHA
};

/// `MAJOR.MINOR.PATCH`(앞자리 0 없는 수 셋).
const fn is_release_version(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    let mut parts = 0;
    while parts < 3 {
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let len = i - start;
        if len == 0 || (len > 1 && b[start] == b'0') {
            return false;
        }
        parts += 1;
        if parts < 3 {
            if i >= b.len() || b[i] != b'.' {
                return false;
            }
            i += 1;
        }
    }
    i == b.len()
}

/// 소문자 16진수 40자리.
const fn is_full_sha(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 40 {
        return false;
    }
    let mut i = 0;
    while i < b.len() {
        if !matches!(b[i], b'0'..=b'9' | b'a'..=b'f') {
            return false;
        }
        i += 1;
    }
    true
}

const _: () = assert!(
    RELEASE_VERSION.is_empty() == SOURCE_SHA.is_empty(),
    "FVOCI_RELEASE_VERSION and FVOCI_SOURCE_SHA must be set together (or both left out for a dev build)"
);
const _: () = assert!(
    RELEASE_VERSION.is_empty() || is_release_version(RELEASE_VERSION),
    "FVOCI_RELEASE_VERSION must be MAJOR.MINOR.PATCH (the release tag without the leading v)"
);
const _: () = assert!(
    SOURCE_SHA.is_empty() || is_full_sha(SOURCE_SHA),
    "FVOCI_SOURCE_SHA must be the full 40-character lowercase commit SHA"
);

/// 실행 중인 빌드. 제품 버전과 소스 SHA만 담는다.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct BuildInfo {
    /// 제품 버전(`0.0.1`) 또는 개발 빌드면 `dev`.
    pub version: &'static str,
    /// 소스 커밋 전체 SHA 또는 개발 빌드면 `dev`.
    pub source: &'static str,
}

/// 이 바이너리의 빌드 정보.
pub const CURRENT: BuildInfo = BuildInfo {
    version: VERSION,
    source: SOURCE,
};

#[cfg(test)]
mod tests {
    use super::{is_full_sha, is_release_version};

    #[test]
    fn release_versions_and_shas_are_checked() {
        assert!(is_release_version("0.0.1"));
        assert!(is_release_version("10.2.30"));
        for bad in [
            "",
            "0.0",
            "v0.0.1",
            "0.0.1-rc1",
            "01.0.0",
            "0.0.1.2",
            "0..1",
        ] {
            assert!(!is_release_version(bad), "{bad}");
        }
        assert!(is_full_sha("96de6afb3519d01840f1366eac8936c9afd70be1"));
        assert!(!is_full_sha("96de6af"));
        assert!(!is_full_sha("96DE6AFB3519D01840F1366EAC8936C9AFD70BE1"));
    }
}
