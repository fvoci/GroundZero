# BeautifulRust rust-best-practices 지침

Rust 코드를 쓰거나 고칠 때 에이전트 세션이 참고하는 스킬이다(D-130 개정 2026-10-27). 스킬 본문은 [SKILL.md](SKILL.md), 다섯 지침의 요약은 [references/](references/)에 있다. 리뷰용 짝은 [rust-review](../rust-review/)다.

- 출처: [morganlinton/BeautifulRust](https://github.com/morganlinton/BeautifulRust) `skills/rust-best-practices/`(`SKILL.md`, `references/*.md`), 커밋 `c6f56d27a01739d404da2a554daaea023d7ddcca`. 평가용 예제(`evals/`)는 가져오지 않았다.
- 라이선스: MIT, 같은 폴더의 [LICENSE](LICENSE). `references/`는 BeautifulRust가 각 원본 지침(Google Rust Style Guide, Rust API Guidelines, Effective Rust, Rust Design Patterns, Microsoft Pragmatic Rust Guidelines)을 요약한 것이고, 원본 링크는 SKILL.md 끝에 있다.
- 바꾼 것: 원문 문장은 고치지 않았다. 저장소 서식 검사(Prettier)가 바꾼 표기와, 스킬 로더가 읽도록 머리말 바로 뒤에 덧붙인 "fvoci 저장소에서 쓸 때" 상자(원문 아님)뿐이다.
- 갱신: 원본이 바뀌어도 자동으로 따라가지 않는다. 갱신하려면 새 커밋에서 다시 가져와 이 파일의 커밋 SHA를 고치고 PR에서 검토한다.

## 이 저장소에서 적용할 때

- 저장소 규칙이 우선한다: [AGENTS.md](../../../AGENTS.md), [결정 기록](../../../docs/decisions.md), 저장소의 rustfmt·Clippy 설정(D-09).
- `crates/server`와 `xtask`는 애플리케이션 코드다. 라이브러리 공개 API 규칙은 엄격하게 적용하지 않는다.
- SKILL.md가 권하는 `[lints]`·`lints/` 설정 도입은 검사 규칙 변경이므로 사용자 결정으로만 한다.
- 이름이 같은 BeautifulRust 플러그인 스킬(`beautiful-rust:rust-best-practices`)이 설치된 머신에서는 이 저장소 스킬을 쓴다(위 예외가 들어 있음).
