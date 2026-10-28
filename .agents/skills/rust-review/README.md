# BeautifulRust rust-review 지침

Rust 변경을 검토할 때 CodeRabbit(`.coderabbit.yaml`의 코딩 지침)과 에이전트 세션이 함께 참고하는 스킬이다(D-130 개정 2026-10-27). 스킬 본문은 [SKILL.md](SKILL.md)다.

- 출처: [morganlinton/BeautifulRust](https://github.com/morganlinton/BeautifulRust) `skills/rust-review/SKILL.md`, 커밋 `c6f56d27a01739d404da2a554daaea023d7ddcca`.
- 라이선스: MIT, 같은 폴더의 [LICENSE](LICENSE). 원문 문장은 고치지 않았다. 저장소 서식 검사(Prettier)가 보고서 예시의 코드 울타리 표기(`~~~` → ` ```` `)와 빈 줄만 바꿨고, 스킬 로더가 읽도록 머리말 바로 뒤에 "fvoci 저장소에서 쓸 때" 상자를 덧붙였다(원문 아님).
- 갱신: 원본이 바뀌어도 자동으로 따라가지 않는다. 갱신하려면 새 커밋에서 다시 가져와 이 파일의 커밋 SHA를 고치고 PR에서 검토한다.

## 이 저장소에서 적용할 때

- 저장소 규칙이 우선한다: [AGENTS.md](../../../AGENTS.md), [결정 기록](../../../docs/decisions.md), 저장소의 rustfmt·Clippy 설정(D-09). 충돌하면 저장소 규칙을 따른다.
- `crates/*`와 `xtask`는 애플리케이션 코드다. 라이브러리 공개 API 규칙(semver, `#[non_exhaustive]`, sealed trait 등)은 엄격하게 적용하지 않는다.
- 지침 5절의 `[lints]` 표 도입이나 `clippy::pedantic` 전체 적용은 범위 밖 제안이므로 지적으로 올리지 않는다. 검사 규칙 변경은 사용자 확인 사항이다(AGENTS.md).
- 이름이 같은 BeautifulRust 플러그인 스킬(`beautiful-rust:rust-review`)이 설치된 머신에서는 이 저장소 스킬을 쓴다(위 예외가 들어 있음).
