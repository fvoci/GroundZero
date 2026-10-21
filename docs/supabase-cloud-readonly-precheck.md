# Supabase Cloud 읽기 전용 사전 점검 준비

상태: **비활성 코드의 Draft PR 게시 승인, 자격 증명 사용 미승인, 원격 점검 NOT RUN**. 기준 main은
`90edd23137259d999982eab10b65f4016d573c3f`(2026-10-21 확인)다. 별도 로컬 브랜치
`prepare/supabase-cloud-readonly`에서 작성했다. #39–#41의 파일·브랜치는 수정하지 않는다.

사용자는 버려도 되는 전용 Cloud 프로젝트의 Session pooler 5432 URL을 repository Actions secret
`SUPABASE_TEST_DATABASE_ADMIN_URL_SESSION_POOLER`로 등록했다고 알렸다. 확인된 것은 등록 화면의
이름뿐이다. 값·연결·TLS·권한은 확인하지 않았다. 이번 변경은 비활성 코드의 commit·push·Draft PR
게시까지만 승인됐다. Secret 조회·추출, DB 연결, 수동 workflow 실행, Ready 전환·main 병합·배포는
하지 않는다. 사용자 Mac은 사용하지 않는다.

## 범위와 현재 코드의 근거

- [결정 D-88](decisions.md)의 기존 지원 범위는 셀프호스트 Supabase다. 이번 별도 사전 점검 준비는
  Supabase Cloud 제품 지원이나 마이그레이션·복구 지원을 확정하지 않는다.
- 기존 [ci.yml](../.github/workflows/ci.yml)의 `supabase` 작업과
  [xtask/src/supabase.rs](../xtask/src/supabase.rs)는 로컬 격리 컨테이너를 사용한다.
  `cargo xtask test --supabase`는 bootstrap·migrate·전체 통합 테스트를 수행하므로 원격 URL을
  전달하지 않는다. 기존 PR CI와 테스트 진입점은 바꾸지 않는다.
- [db/admin.rs](../crates/server/src/db/admin.rs)의 `fvoci_migrator`, `fvoci_app`, `fvoci`와 역할
  속성·멤버십 확인, [backup.rs](../crates/server/src/backup.rs)의 `session_replication_role`,
  [개발 문서 85절](development.md#85-앱이-만드는-uuidv7과-셀프호스트-supabase-db-2026-10-20-d-88-featsupabase-selfhostfeatapp-uuidv7)의
  UUID 계약이 카탈로그 항목 선정 근거다. 이 제품 코드를 실행하거나 관리 기능을 재사용하지 않는다.
- 저장소의 `test-docker-ci` 스킬은 일반 검사에서 `cargo xtask check/test`를 쓰게 한다. 이번 사용자의
  명시적 범위(작은 별도 읽기 전용 스크립트, 기존 PR CI 불변)에 따라 새 점검과 오프라인 시험만
  독립 진입점으로 둔다. 일반 CI 규칙을 완화하거나 검사 제외를 추가하지 않는다.

## 실행 경로는 아직 비활성

[supabase-cloud-readonly.yml](../.github/workflows/supabase-cloud-readonly.yml)은 `workflow_dispatch`
전용이다. 작업 조건의 **리터럴 `false`** 때문에 게시되더라도 실행할 수 없다. 입력이나 repository
variable로 이 조건을 켤 수 없다. 향후 자격 증명 사용과 대상·코드에 관한 별도 승인을 받은 뒤,
검토 가능한 변경으로 이 조건을 바꾸어야 한다. 이번 Draft PR에서는 그 활성화 변경을 하지 않는다.
오프라인 시험도 이 비활성 조건을 확인하므로, 향후 활성화 승인 diff에서는 해당 회귀 가드의 변경을
함께 검토해야 한다.

활성화 검토 시에도 유지할 제한:

- 저장소 `fvoci/GroundZero`, `refs/heads/main`, 수동 이벤트만 허용한다. checkout은 dispatch의
  `github.sha`로 고정하고 임의 ref·repository·명령 입력은 없다. 외부 PR 코드를 Secret과 실행하는
  `pull_request_target`, `workflow_run`, `workflow_call` 등은 없다.
- GitHub token은 `contents: read`, checkout token 저장은 끈다. checkout action은 v4.2.2의 전체 SHA로
  고정했다. 전역 동시 실행 그룹을 쓰고 진행 중인 작업을 취소하지 않는다. 작업 제한은 3분이다.
- Secret은 마지막 점검 step에서만 주입한다. 앞선 오프라인 시험에는 주입하지 않는다. Secret 이름은
  위 한 개만 참조한다. 아티팩트 업로드·캐시·DB dump·디버그 환경 출력은 없다.
- 호스트의 Python 3 표준 라이브러리와 `psql`만 사용한다. 패키지 설치 단계나 외부 SQL 다운로드는 없다.
  필요한 실행체·CA 파일이 없으면 실패한다. 제품 서버·Compose·통합 테스트는 실행하지 않는다.

## 프로젝트 확인과 대상 제한

`ADMIN`은 이 Secret의 **계정 용도**다. 연결 방식은 항상 `session_pooler`로 기록하며 direct 성공으로
바꾸지 않는다. 이 좁은 준비본은 기본 관리 계정 `postgres.<project-ref>`, Cloud shared pooler의 정확한
호스트, 명시적인 `5432`, DB `postgres`만 받는다. custom role·다른 DB·다른 endpoint는 별도 검토 대상이다.
6543(transaction), `db.<project-ref>.supabase.co`(direct), 여러 호스트, URI 옵션을 통한 대상·TLS 우회는 거절한다.

실제 프로젝트 ref나 호스트를 코드에 넣거나 지금 사용자에게 요구하지 않는다. **사용자는 별도 지문
계산 도구를 다루지 않는다.** 실제 실행 전 승인할 것은 Dashboard의 해당 **전용 프로젝트 식별자**와
Connect의 **Session pooler 대상**, 이 코드의 읽기 전용 자격 증명 사용이다. 프로젝트 식별자는 비밀번호는
아니지만 사용자가 연결 정보를 가렸으므로 불필요하게 로그에 남기지 않는다. 지역명으로 호스트를 추정하지 않는다.

승인된 프로젝트 ref와 정확한 호스트를 기반으로 담당자가 내부 비교값을 준비한다. 최소 추가 설정은
저장소 변수 `SUPABASE_TEST_TARGET_SHA256` 하나이고, 수동 실행 입력은
`confirm_disposable_target` 확인 체크 하나다. 지금은 변수 등록·workflow 활성화를 하지 않는다.
아래는 구현 검토용 형식이며 사용자에게 계산·복사를 요구하는 절차가 아니다.
내부 비교값은 아래 UTF-8 문자열의 SHA-256(소문자 hex 64자)이다. 각 줄은 LF이며 마지막 줄 뒤에도 LF가 있다.

```text
supabase-cloud-readonly-v1
<승인한 project-ref>
<Connect에서 확인한 정확한 소문자 pooler 호스트>
postgres
5432
postgres
```

스크립트는 실행 확인이 참이고, 저장소 승인 지문과 URL에서 엄격히 해석한 프로젝트·호스트의 지문이
일치해야 네트워크 실행체를 시작한다. 승인 지문은 Secret에서 자동 등록하지 않는다.
누락·잘못된 형식·불일치는 모두 실패다. **현재 실제 대상이 검증됐다는 뜻은 아니다.**

지문은 비밀번호도 Secret 대체물도 아니다. 저장소 설정·step 환경 메타데이터에는 지문이 보일 수 있다.
원문 ref·호스트·사용자·DB URL을 넣는 실행 입력란은 없다. GitHub 변수와 workflow를 수정할 수 있는 사람은
신뢰 경계 안에 있으므로 이 제한을 악의적인 저장소 관리자로부터의 보호라고 주장하지 않는다.
PostgreSQL 자체에 Supabase project-ref를 증명하는 표준 카탈로그가 있는 것도 아니다. 대상 확인은
독립 승인 지문, TLS로 검증한 pooler endpoint, 인증 사용자 suffix와 pooler 라우팅에 의존한다.

## TLS와 공식 CA

Supabase의 [연결 방식 문서](https://supabase.com/docs/guides/database/connecting-to-postgres)는
Session pooler 5432와 transaction pooler 6543을 구분한다. direct도 5432를 사용하므로 포트만 확인하지 않는다.
[PSQL 공식 예제](https://supabase.com/docs/guides/database/psql)는 Session pooler에
`sslmode=verify-full`과 `sslrootcert`를 지정한다.

스크립트는 libpq의 `verify-full`을 강제하여 인증서 체인과 호스트 이름을 확인하고 GSS 암호화 경로를
끄며, `/etc/ssl/certs/ca-certificates.crt`를 명시한다. 이 파일은 러너 OS의 CA bundle이다.
인증서 검증 실패 시 재시도·`require`로 하향·검증 생략을 하지 않는다. `pg_stat_ssl`의 backend 정보로
클라이언트→pooler TLS 성공을 대신 판정하지 않는다.

별도 Cloud CA가 필요한 경우 현재 구현은 멈춘다. 공식 배포 출처는 해당 프로젝트
**Dashboard → Database Settings → SSL Configuration → Download Certificate**다.
[공식 SSL 설정 안내](https://supabase.com/docs/guides/platform/ssl-enforcement)를 따른다.
현재 확인한 문서에는 모든 프로젝트에 쓸 정적 공개 CA 다운로드 URL이 없으므로 임의 URL을 만들지 않는다.
승인 후 해당 출처·프로젝트·인증서 fingerprint·유효 기간을 확인하고, 검토된 CA 파일의 명시적
`sslrootcert` 경로와 배포 방식을 별도 변경으로 추가할 수 있다. 서버가 보낸 인증서나 임의 인증서를
실패 해결 목적으로 자동 신뢰하지 않는다. Supabase 네트워크·SSL enforcement 설정도 바꾸지 않는다.

## 읽기와 출력의 경계

[cloud_readonly_precheck.py](../deploy/supabase/cloud_readonly_precheck.py)는 `--connect`가 없으면
Secret을 읽지 않고 `NOT RUN`을 출력한다. 연결 실행 시에도 single-host 단일 `psql` 프로세스와 한 연결만
사용하고 retry하지 않는다. 연결 5초, 문장 5초, idle transaction 5초, 전체 프로세스 20초 제한이다.
URI·비밀번호는 명령 인자로 넘기지 않는다. 깨끗한 child 환경, 격리된 HOME, `psql -X -w`로 사용자
설정·기존 libpq 환경·비밀번호 프롬프트의 영향을 막는다.

SQL은 코드에 고정되어 있으며 기본 read-only 세션의 `BEGIN READ ONLY` 안에서 카탈로그를 읽고
`ROLLBACK`한다. PostgreSQL 버전 번호, 현재 계정·DB가 예상값인지, 역할 속성·기존 fvoci 역할과 스키마,
필요한 함수·설정의 카탈로그만 다룬다. 계정·DB 이름 원문은 출력하지 않고 일치 여부로 바꾼다.
카탈로그의 기존 권한 표시는 실제로 그 권한을 행사했다는 뜻이 아니다.
특히 `has_parameter_privilege(... session_replication_role, SET)`는 PostgreSQL의 기본 ACL 관찰이며
supautils가 별도로 허용하는지, 복구가 가능한지는 입증하지 못한다.

최종 JSON은 다음 영역을 구분한다.

- 연결 종류와 연결/TLS 확인: `session_pooler`, 인증서·호스트 검증을 통과한 연결 여부.
- 읽기 전용 카탈로그: 고정 키의 bool·정수만. 임의 문자열·카탈로그 원문은 내보내지 않는다.
- direct·변경 시험: `NOT RUN`. bootstrap, migration, seed, restore, 역할·비밀번호 변경,
  쓰기·복구 지원, 전체 원격 통합 시험을 성공으로 표시하지 않는다.

stdout/stderr를 먼저 포착하고, psql 오류·예외·URI·호스트·사용자·비밀번호 원문은 재출력하지 않는다.
실패 원인은 고정 분류로만 보고한다. 연결 실패만으로 인증 실패와 TLS 실패 등을 정확히 구별할 수
없으면 미검증으로 남긴다. SQL 실패 후 연결 성공 근거가 있더라도 카탈로그 성공으로 올리지 않는다.
실행 중인 runner의 관리자·OS가 프로세스 자격 증명을 볼 수 있는 일반적 경계는 남는다.

## 로컬 검증

이 준비본의 시험 진입점은 로컬과 수동 workflow에서 같다.

```sh
python3 -I -B -m unittest discover -s deploy/supabase -p test_cloud_readonly_precheck.py
python3 -I -B deploy/supabase/cloud_readonly_precheck.py
```

시험은 비밀 없는 가짜 입력, 임시 fake psql과 프로세스 경계 대체만 사용한다. 실제 DNS·DB·TLS
세션을 만들지 않는다. 테스트 자격 증명은 `deploy/supabase/test_cloud_readonly_precheck.py`의
명백한 시험 값뿐이다. TLS 옵션 설정·오류 차단 검증과 실제 Cloud TLS 성공은 구분한다.

검증 대상은 위 main SHA에서 시작한 로컬 작업 diff다(2026-10-21).

| 검사                                          | 결과                                                                                   |
| --------------------------------------------- | -------------------------------------------------------------------------------------- |
| 위 `unittest` 명령                            | PASS, 15건. 악성 URI·대상 불일치·설정 누락·가짜 TLS 오류·시간 초과·출력 누출 차단 포함 |
| 기본 CLI (`--connect` 없음)                   | PASS, 연결·TLS·카탈로그·변경 시험 `NOT RUN`, 종료 코드 2                               |
| `actionlint` 1.7.7, 새 workflow만             | PASS. 공식 릴리스 checksum 확인 후 임시 도구로 정적 검사                               |
| 저장소 Prettier 3.9.10, 새 YAML·문서만        | PASS                                                                                   |
| `git diff --check`                            | PASS                                                                                   |
| `cargo xtask check`                           | NOT RUN, 이 Cloud 환경에 cargo 없음. Rust·제품 코드는 변경하지 않음                    |
| 실제 `psql` SQL 실행·Cloud 연결·TLS·권한      | NOT RUN, psql 없음·실제 연결 미승인. 설치하지 않음                                     |
| 변경·복구·전체 통합 시험·GitHub workflow 실행 | NOT RUN, 이번 범위 밖                                                                  |

psql 오류는 합성한 값으로만 시험했다. 실제 인증서 검증 실패나 실제 쿼리 시간 초과를 원격에서
재현한 결과가 아니다. CA 추가 필요 여부와 프로젝트·계정의 실제 권한도 아직 모른다.

## 추가 공식 근거

- [Pooler 호스트·사용자 오류](https://supabase.com/docs/guides/troubleshooting/tenant-or-user-not-found):
  정확한 호스트를 Connect에서 확인해야 한다.
- [Supabase 역할과 superuser 제한](https://supabase.com/docs/guides/database/postgres/roles-superuser):
  `ADMIN` 이름만으로 superuser라고 추정하지 않는다.
- [libpq 환경 변수](https://www.postgresql.org/docs/current/libpq-envars.html),
  [psql 옵션](https://www.postgresql.org/docs/current/app-psql.html): 입력·설정·실패 제어의 근거.

공식 웹 문서 확인일: 2026-10-21. 서비스 제약이나 인증서가 바뀌면 실행 승인 전에 다시 확인한다.
