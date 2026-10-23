# SQL 준비 실행 증거의 분리

2026-10-23, 대상 체크아웃 `abd545a250ff1fd9cc166b3c078c4dff76846724` (#92).
이 문서는 증거 배치와 이번 분리 검증의 요약이다. SQL 동작 검증은 새로 실행하지 않았다.

## 현재 기준과 역사 기록

현재 사용자 요구사항, 승인된 결정, 현재 코드·테스트가 기준이다. 결정과 코드가 충돌하면
어느 쪽도 조용히 우선하지 말고 보고한다. 과거 기록은 **역사 기록이며 현재 상태를 나타내지 않음**이며,
관련 결함 또는 SHA를 조사할 때만 명시적으로 접근한다. 당시 컨테이너 상태·PR 상태·실패 로그를 현재 상태로 읽지 않는다.

- 유지: 제품 코드, D-101~D-106 결정·구현, 누적 마이그레이션, 실제 후보 SQL, 비교 도구, 카탈로그 조회 SQL,
  재현 fixture와 명령, SHA별 결과 요약, 테이블 요약, 생성 계약·API 클라이언트·고지·자산.
- 분리: `inventory/*.jsonl` 8개와 `verification/runs/`, `verification/diagnostics/`,
  `verification/container-disposition.txt`의 실행별 로그·카탈로그·환경/소스 스냅숏·체크섬 등 총 270개.
  스크린샷이나 별도 중간 소스 파일은 이번 추적 파일 집합에 없었다. 앞으로도 이런 원시 출력은 `target/`에 둔다.
- 종전 장문 실행 경과는 원격에 남기고 [DB 결과](verification/results.md)와 [품질 결과](quality-check.md)는
  마지막 기록된 결과·대체 관계·NOT RUN 경계를 남겼다. 보존되지 않은 당시 로컬 `target/` 파일까지 원격에 있다고 주장하지 않는다.

## 기존 원격 보존 위치와 무결성

기존 `origin`의 `fvoci/sql-prep-review-head` 브랜치와 [검토 완료 #106](https://github.com/fvoci/GroundZero/pull/106)을 그대로 유지한다.
새 공개 저장소·태그를 만들거나 접근 권한을 바꾸지 않았다. #106을 재검토하거나 병합하지 않는다.

- 보존 커밋: [`0dfa2b9f99cbcafb71158bdfe8598dba985d530b`](https://github.com/fvoci/GroundZero/tree/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation).
- 전체 tree: `d890ac607e857e080bbe05e54da385ec19e7fca9`. 대상 `abd545a`의 전체 tree와 같다.
- [목록 원본](https://github.com/fvoci/GroundZero/tree/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/inventory),
  [실행 원본](https://github.com/fvoci/GroundZero/tree/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/runs),
  [실패 진단 원본](https://github.com/fvoci/GroundZero/tree/0dfa2b9f99cbcafb71158bdfe8598dba985d530b/preparation/sql-consolidation/verification/diagnostics).
- 전체 분리 파일의 [SHA-256 목록](evidence.sha256): 270개, 3,738,386 bytes.
  목록 자체 SHA-256: `765486c09302265c217a507a7503ba5dc6325d82ec4c8a50004366c0c589dec2`.
- 삭제 **전에** `git ls-remote`로 브랜치 SHA를 확인하고, 로컬 객체를 공유하지 않는 임시 bare 저장소에
  원격에서 `fetch --depth=1`했다. `git fsck --full`과 tree 동일성을 확인한 뒤 원격에서 받은 270개 blob의
  바이트 및 SHA-256을 작업 트리와 각각 비교했다. 모두 PASS(2026-10-23 11:33 UTC).
  이 목록은 기존 로그에 기록된 더 이전 원본 해시와 별개로, 원격에 실제 커밋된 바이트를 식별한다.

필요할 때만 보존본을 확인한다. 현재 체크아웃에 원격 트리를 풀지 않고 해시를 다시 확인하는 예:

```sh
set -euo pipefail
proof=$(mktemp -d)
evidence_sha=0dfa2b9f99cbcafb71158bdfe8598dba985d530b
git init --bare "$proof/evidence.git"
git -C "$proof/evidence.git" fetch --depth=1 https://github.com/fvoci/GroundZero.git refs/heads/fvoci/sql-prep-review-head
test "$(git -C "$proof/evidence.git" rev-parse FETCH_HEAD)" = "$evidence_sha"
while read -r expected file; do
  actual=$(git -C "$proof/evidence.git" show "$evidence_sha:$file" | sha256sum | cut -d ' ' -f 1)
  test "$actual" = "$expected" || { printf 'FAIL %s\n' "$file"; exit 1; }
done < preparation/sql-consolidation/evidence.sha256
```

## 이번 분리 검증과 기존 실행의 구분

| 범위                                                       | 상태    | 근거·한계                                                                                          |
| ---------------------------------------------------------- | ------- | -------------------------------------------------------------------------------------------------- |
| 원격 보존·바이트 무결성                                    | PASS    | 삭제 전 독립 fetch, tree 동일, 270/270 SHA-256 일치                                                |
| 문서 포맷·링크                                             | PASS    | 수정 Markdown에 Prettier 검사; 로컬 링크와 원격 SHA 경로 존재 확인                                 |
| 기본 검색·Git 색인                                         | PASS    | `rg --files`와 `git ls-files`에 분리 원본 0개; 파일도 작업 트리에 없음                             |
| 새 실행 출력 제외                                          | PASS    | 기존 `.gitignore`, `.prettierignore`, ESLint의 `target/` 제외를 실제 표식 파일로 확인              |
| 시험 수집                                                  | PASS    | Vitest `list --filesOnly --json` 135개, Cargo `metadata --no-deps` 47개 대상에 원시 증거·표식 없음 |
| 제품·마이그레이션·후보·도구·fixture 입력                   | PASS    | 시작 SHA 대비 diff 없음(SQL/YAML fixture 포함; 재현 README의 링크와 비교/보관 설명만 갱신)         |
| SQL A/B·전체 품질 검사                                     | NOT RUN | 이번 작업은 증거 분리이며 동작 변경 없음; 과거 PASS로 대신하지 않음                                |
| 기존 데이터·백업·SQLx 이력 전환, Cloud, transaction pooler | NOT RUN | 종전 한계 유지; session 결과의 FreshDb direct 연결 예외도 유지                                     |

이번 작업의 미해결 FAIL은 없다. 과거 중간 FAIL과 대체된 실행은 보존 커밋의 기록을 따른다.
마지막 A/B 실행 소스는 `f5b5a9d4a42dc6c87deddddcda0d0ea41f7092f9`, 후보 생성 기준은
`3db81d1deae24b16478581ebd6a06de2b2a2d8c9`이며 [상세 요약](verification/results.md)의 범위에만 적용한다.
이번 분리가 그 실행의 재검증은 아니다.

링크 검사는 준비 문서와 추가한 안내의 로컬 링크 71개·보존 SHA 링크 37개를 확인했다.
기존 개발 문서의 산문 `](원래대로)` 한 곳은 변경 범위 밖이며 링크 판정에서 제외했다.

`.gitignore`만 추가해서 추적 파일을 숨긴 것이 아니다. 원시 파일을 기본 트리·색인에서 제거했고 새 출력은
이미 모든 관련 도구가 제외하는 `target/`을 사용한다. CodeGraph 색인은 이 체크아웃에 없으며 새로 만들지 않았다.
추후 색인도 현재 트리를 대상으로 하고 외부 보존본이나 `target/`을 명시적으로 포함하지 않는다.
기존 제품·테스트 검사 규칙을 바꾸거나 새 필수 관문·설정 파일을 추가하지 않았다.
