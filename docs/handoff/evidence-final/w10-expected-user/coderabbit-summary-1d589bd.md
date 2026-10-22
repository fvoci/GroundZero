<!-- coderabbit-ai-generated:v1 -->

<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/fvoci/GroundZero/pull/60?scope=<redacted>&amp;cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

<!-- review_stack_entry_end -->
<!-- recent_review_start -->

No actionable comments were generated in the recent review. 🎉

<details>
<summary><strong>ℹ️ Recent review info</strong></summary>
<dl>
<dd>

<details>
<summary><strong>⚙️ Run configuration</strong></summary>
<dl>
<dd>

- **Configuration used**: Repository: fvoci/GroundZero/.coderabbit.yaml
- **Review profile**: CHILL
- **Plan**: Advanced
- **Run ID**: `cb8cea2c-a43b-47ee-9974-1a661f25f20e`

</dd>
</dl>
</details>

<details>
<summary><strong>📥 Commits</strong></summary>
<dl>
<dd>

Reviewing files that changed from the base of the PR and between 0345026665c9514099e3835294b635c96cf6baa7 and 1d589bdd4ea7e3e92d9cc6b00dca9ee9932c63a9.

</dd>
</dl>
</details>

<details>
<summary><strong>⛔ Files ignored due to path filters: 1</strong></summary>
<dl>
<dd>

* `packages/api-client/src/generated/schema.ts` is excluded by `!**/generated/**`

</dd>
</dl>
</details>

<details>
<summary><strong>📒 Files selected for processing: 21</strong></summary>
<dl>
<dd>

* `apps/web/messages/common/en.json`
* `apps/web/messages/common/ko.json`
* `apps/web/messages/documents/en.json`
* `apps/web/messages/documents/ko.json`
* `apps/web/src/lib/api/client.ts`
* `apps/web/src/lib/api/errors.ts`
* `apps/web/src/lib/features/documents/DocumentEditor.svelte`
* `apps/web/src/lib/features/documents/LiveDocumentEditor.svelte`
* `apps/web/src/lib/features/home/HomeDashboard.svelte`
* `apps/web/src/lib/features/notes/NotesScreen.svelte`
* `apps/web/tests/component/document-save-account-changed.test.ts`
* `apps/web/tests/component/etag.test.ts`
* `contracts/openapi.json`
* `crates/server/src/error.rs`
* `crates/server/src/features/accounts.rs`
* `crates/server/src/features/documents.rs`
* `crates/server/src/http/mod.rs`
* `crates/server/tests/cache.rs`
* `crates/server/tests/common/mod.rs`
* `crates/server/tests/expected_user.rs`
* `docs/development.md`

</dd>
</dl>
</details>

<details>
<summary><strong>🚧 Files skipped from review as they are similar to previous changes: 2</strong></summary>
<dl>
<dd>

* apps/web/messages/common/en.json
* apps/web/messages/common/ko.json

</dd>
</dl>
</details>

**Included review availability:** This review used your included allowance. Your plan provides up to 10 included reviews per hour; 6 remain after this review.

</dd>
</dl>
</details>





<!-- recent_review_end -->
<!-- walkthrough_start -->

<details>
<summary><strong>📝 Summary</strong></summary>
<dl>
<dd>

<!-- This is an auto-generated comment: release notes by coderabbit.ai -->

## Summary by CodeRabbit

* **새 기능**
  * 앱 시작 실패 시 번역된 오류 안내와 재시도 버튼을 제공하며, 서버 연결이 복구되면 자동으로 다시 시도합니다.
  * 세션 만료 시 새 탭 로그인 안내를 표시하고, 계정 전환이 감지되면 화면을 새로고침합니다.
  * 감사 로그에 보관, 템플릿, 실시간 공동 편집 동작과 상태를 번역해 표시합니다.

* **버그 수정**
  * 계정 전환 후 이전 계정의 문서나 노트가 저장되지 않도록 보호하고, 미저장 편집 내용은 해당 계정의 초안으로 유지합니다.
  * 파일 이름 변경, 멤버 초대, 그룹 생성, 보존 기간 설정의 오류를 관련 입력란에 표시하고 실패한 입력값을 유지합니다.
  * 멤버 변경 충돌 시 목록을 새로 불러오며, 저장된 보기의 중복 삭제 요청을 방지합니다.
  * 요청 크기 초과 및 예상치 못한 오류 안내를 개선했습니다.

* **문서**
  * 앱 시작 실패 및 요청 크기 초과 오류 처리 안내를 업데이트했습니다.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
앱 시작·세션 오류 처리, 감사 로그 라벨, 입력 요청 상태를 변경했습니다. 문서 저장과 노트 생성에 기대 사용자 검증을 추가하고, 계정이 바뀌면 저장을 중단해 기존 계정의 초안을 유지합니다.

**Changes: 앱 시작 및 세션 오류**

|Layer / File(s)|Summary|
|:---|:---|
|**세션 만료 상태와 계정 확인** <br> `apps/web/src/lib/features/feedback/*`, `apps/web/src/lib/api/client.ts`, `apps/web/messages/common/*.json`, `apps/web/tests/component/session-expired.test.ts`|세션 필요 API의 401 응답을 추적합니다. 만료 배너와 새 탭 로그인 링크를 표시하고, 창 포커스 복귀 시 계정을 확인합니다.|
|**앱 시작 오류와 API 오류 문구** <br> `apps/web/src/app/App.svelte`, `apps/web/src/lib/api/errors.ts`, `apps/web/messages/app/*.json`, `apps/web/messages/common/*.json`, `apps/web/src/lib/features/settings/DisplaySettingsDialog.svelte`, `apps/web/tests/component/app-start-error.test.ts`, `apps/web/tests/component/error-text-paths.test.ts`, `docs/development.md`|시작 오류에 번역 메시지와 수동·자동 재시도를 추가합니다. API 오류 코드를 번역 문구로 변환하고 표시 설정 저장에 `errorText`를 사용합니다.|

**감사 로그 라벨**

|Layer / File(s)|Summary|
|:---|:---|
|**감사 항목 변환과 렌더링** <br> `apps/web/src/lib/features/audit/labels.ts`, `apps/web/src/lib/features/audit/AuditScreen.svelte`, `apps/web/messages/audit/*.json`, `apps/web/tests/component/audit-codes.test.ts`|감사 작업 라벨과 상태·역할·공개 범위 값 변환을 추가합니다. 감사 화면은 변환된 주제를 표시합니다.|

**입력 오류와 요청 상태**

|Layer / File(s)|Summary|
|:---|:---|
|**첨부 파일 이름 변경** <br> `apps/web/src/lib/features/files/AttachmentsPanel.svelte`, `apps/web/tests/component/file-rename.test.ts`|중복 제출을 막습니다. 실패 시 입력값과 오류를 유지하고, 오래된 응답이 새 편집 상태를 변경하지 않도록 합니다.|
|**멤버 입력과 충돌 갱신** <br> `apps/web/src/lib/features/members/MembersScreen.svelte`, `apps/web/tests/component/members-*.test.ts`|초대와 그룹 생성 실패를 필드에 표시하고 입력값을 유지합니다. 409 응답에서는 멤버 상태를 다시 불러옵니다.|
|**보존 기간 필드별 오류** <br> `apps/web/src/lib/features/trash/TrashScreen.svelte`, `apps/web/tests/component/trash-retention-field.test.ts`|검증·저장 오류를 각 필드에 표시합니다. 이전 요청의 늦은 실패가 최신 결과를 덮지 않도록 처리합니다.|
|**저장된 보기 삭제** <br> `apps/web/src/lib/features/tasks/TaskBoard.svelte`, `apps/web/tests/component/saved-view-delete.test.ts`|삭제 중 재실행을 막고 삭제 버튼을 비활성화합니다.|

**문서 계정 전환 검증**

|Layer / File(s)|Summary|
|:---|:---|
|**서버 계약과 기대 사용자 검증** <br> `contracts/openapi.json`, `crates/server/src/error.rs`, `crates/server/src/features/accounts.rs`, `crates/server/src/features/documents.rs`, `crates/server/src/http/mod.rs`, `crates/server/tests/expected_user.rs`, `crates/server/tests/common/mod.rs`, `crates/server/tests/cache.rs`, `docs/development.md`|문서 저장과 개인 노트 생성에 필수 `x-fvoci-expected-user` 헤더를 적용합니다. 세션 사용자와 다르면 쓰기 전에 409를 반환하고, 저장 409 응답 스키마를 확장합니다.|
|**웹 편집 계정 고정과 저장 차단** <br> `apps/web/src/lib/api/client.ts`, `apps/web/src/lib/features/documents/*`, `apps/web/src/lib/features/home/HomeDashboard.svelte`, `apps/web/src/lib/features/notes/NotesScreen.svelte`, `apps/web/messages/documents/*.json`, `apps/web/tests/component/document-save-account*.test.ts`, `apps/web/tests/component/document-save-account-changed.test.ts`, `apps/web/tests/component/account-switch-unload.test.ts`, `apps/web/tests/component/etag.test.ts`, `docs/development.md`|편집 시작 계정을 저장 요청에 전달합니다. 계정 변경이 확인되거나 서버가 `account_changed`를 반환하면 전송을 중단하고 초안을 기존 계정에 유지합니다. 테스트는 자동 저장·직접 저장·재시도와 이탈 경고 동작을 확인합니다.|

<!-- change_assessment_start -->
**Priority:** ➖ Normal

**Estimated code review effort:** 4 (Complex) | ~45 minutes

<!-- change_assessment_commit:"1d589bdd4ea7e3e92d9cc6b00dca9ee9932c63a9" -->
**Change:** Bug fix
<!-- change_assessment_end -->

**Possibly related issues**

- [fvoci/GroundZero#88](https://github.com/fvoci/GroundZero/issues/88): 계정 전환 감지, 저장 보호, 초안 저장과 다시 읽기 동작이 이 PR의 변경과 이어집니다.

**Possibly related PRs**

- [fvoci/GroundZero#59](https://github.com/fvoci/GroundZero/pull/59): `DisplaySettingsDialog.svelte`의 저장 오류를 `errorText`로 변환하는 변경이 이 PR에도 포함되어 있습니다.
- [fvoci/GroundZero#57](https://github.com/fvoci/GroundZero/pull/57): 문서 편집기 초안 보존 흐름을 변경하며, 이 PR은 계정 전환 시 초안 보존 처리를 확장합니다.
- [fvoci/GroundZero#52](https://github.com/fvoci/GroundZero/pull/52): 초대 철회 409 충돌을 다루며, 이 PR은 `MembersScreen`에서 해당 응답 뒤 목록을 다시 읽습니다.

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong>⚪ Minimal</strong> · up to <code>1d589</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **⚪ Minimal** · up to `1d589`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"1d589bdd4ea7e3e92d9cc6b00dca9ee9932c63a9","coveredCommitId":"1d589bdd4ea7e3e92d9cc6b00dca9ee9932c63a9","kind":"reviewed"} -->

Failed draft storage no longer silently permits an account-switch reload to discard edits. The change is mergeable after normal checks; real-browser unload behavior remains for follow-up testing.
<!-- final_review_risk_end -->

</dd>
</dl>
</details>

<details>
<summary><strong>Security architecture risk:</strong> <strong>🔵 Low</strong> · up to <code>1d589</code></summary>
<dl>
<dd>

<!-- architecture_review_start -->
<!-- architecture_review_label
### Security Architecture Review

**Security architecture risk:** **🔵 Low** · up to `1d589`
-->

The change strengthens account consistency for document writes without granting new permissions. Clients and servers must adopt the new required header together. Realtime reconnection behavior remains outside this change, and deployed-client compatibility has not been established.

**Retained concerns**
No architecture-level concerns identified.

<details>
<summary><strong>Security review details</strong></summary>
<dl>
<dd>

**Security Blast Radius**
- _inferred_ — The changed gate covers REST document saves, including live-editor title saves and draft-copy saves, plus personal-note creation. Header manipulation cannot independently expand access to another account or document because the authenticated session remains authoritative and existing write authorization remains in place.

**Security Findings and Attack Paths**
- _inferred_ — The documented realtime reconnect exposure predates this PR: after switching accounts, a reconnection may merge pending edits through the new account when that account also has document write access. The compared live-connection code is unchanged, and the inspected REST caller changes do not establish increased exposure. This is a residual architecture fact, not an introduced PR concern; the complete server-side realtime path was not independently re-traced.

**Trust Boundaries and Controls**
- _observed_ — The attacker-controlled header is an equality-check input, not a credential. Missing sessions are rejected before header comparison. Inspected regression tests assert unchanged document state after mismatches, rejection of invalid or duplicate headers, and preservation of reader and outsider restrictions when using their own identities.

**Resilience and Maintainability Implications**
- _inferred_ — Draft-copy preservation remains two separate transactions. If note creation succeeds and the subsequent save fails, a private note can remain under the original account; the inspected client clears its source draft only after the save succeeds. Same-component retries reuse request identities, and note creation deduplicates by authenticated owner and request ID. No automatic rollback appears in these copy operations, and durable retry identity across reloads was not established. This existing non-atomic design does not establish a new cross-account disclosure.
- _inferred_ — Draft account ownership currently depends on application ordering: user state is initialized during bootstrap, while account switching reloads the page rather than replacing that identity in place. The unload path attempts draft persistence and requests a warning if persistence fails. This supports the inspected production flow but does not guarantee real-browser prompt display or navigation cancellation.

**Hardening Proposals**
- _proposed_ — Separately extend editing-account consistency to realtime reconnections, either by comparing a pinned account during connection establishment or by checking identity before reconnecting. This would address the documented pre-existing gap rather than repair a demonstrated regression in this PR.

</dd>
</dl>
</details>
<!-- architecture_review_end -->

</dd>
</dl>
</details>
<!-- pre_merge_checks_walkthrough_start -->

<details>
<summary><strong>Pre-merge checks: <sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" width="14" height="14" align="middle" alt="Passed" title="Passed"></picture></sup> 4 · <sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-failed-14-v2.svg" width="14" height="14" align="middle" alt="Failed" title="Failed"></picture></sup> 1</strong></summary>
<dl>
<dd>

<strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-failed-14-v2.svg" width="14" height="14" align="middle" alt="Failed" title="Failed"></picture></sup> Failed checks: 1 warning</strong>

|     Check name     |                                              Status                                              | Explanation                                                                                                                                                                                               | Resolution                                                                         |
| :---------------- | :----------------------------------------------------------------------------------------------: | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :--------------------------------------------------------------------------------- |
| Docstring Coverage | ![Warning](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-warning.svg) | Docstring coverage is 59.32% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 59 functions across 24 files. (10 skipped… | Write docstrings for the functions missing them to satisfy the coverage threshold. |

<details>
<summary><strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" width="14" height="14" align="middle" alt="Passed" title="Passed"></picture></sup> Passed checks: 4 passed</strong></summary>
<dl>
<dd>

|         Check name         |                                             Status                                             | Explanation                                                                                   |
| :------------------------ | :--------------------------------------------------------------------------------------------: | :-------------------------------------------------------------------------------------------- |
|         Title check        | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | 제목은 앱, 멤버, 파일, 휴지통, 감사 기록과 관련된 주요 웹 변경을 설명합니다. 기대 사용자 검증 등 일부 변경은 제목에 없지만 제목이 변경사항과 직접 관련됩니다. |
|      Description check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | 설명은 웹 수정, 문서 변경, 기대 사용자 계약, 테스트 결과와 실제 브라우저 테스트 미실시 사항을 변경사항과 직접 연결해 설명합니다.                   |
|     Linked Issues check    | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                      |
| Out of Scope Changes check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                      |

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Docstring Coverage</strong></summary>
<dl>
<dd>

**Explanation**

Docstring coverage is 59.32% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 59 functions across 24 files. (10 skipped: 10 unsupported.)

**Resolution**

Write docstrings for the functions missing them to satisfy the coverage threshold.

</dd>
</dl>
</details>

</dd>
</dl>
</details>

<!-- pre_merge_checks_walkthrough_end -->
<!-- autopilot:start -->
- [ ] <!-- {"checkboxId":"2708ad07-9f24-4260-9c11-7dc76a49f2e3"} --> <strong title="Keep fixing CodeRabbit findings and required CI, and resolving merge conflicts">Autofix</strong> · Keep fixing CodeRabbit findings and required CI, and resolving merge conflicts
<!-- autopilot:end -->
<!-- This is an auto-generated comment: all tool run failures by coderabbit.ai -->

> [!WARNING]
> Some tools did not complete. Review the errors below.
> 
> <details>
> <summary><strong>🔧 ESLint</strong></summary>
> <dl>
> <dd>
> 
> > If the error stems from missing dependencies, add them to the package.json file. For unrecoverable errors (e.g., due to private dependencies), disable the tool in the CodeRabbit configuration.
> 
> <details>
> <summary><strong>apps/web/src/lib/api/client.ts</strong></summary>
> <dl>
> <dd>
> 
> ESLint skipped: missing config or dependency (missing-dependency). The ESLint configuration references a package that is not available in the sandbox.
> 
> </dd>
> </dl>
> </details>
> 
> <details>
> <summary><strong>apps/web/src/lib/api/errors.ts</strong></summary>
> <dl>
> <dd>
> 
> ESLint skipped: the matched ESLint configuration already failed (missing-dependency).
> 
> </dd>
> </dl>
> </details>
> 
> <details>
> <summary><strong>apps/web/src/lib/features/documents/DocumentEditor.svelte</strong></summary>
> <dl>
> <dd>
> 
> ESLint skipped: the matched ESLint configuration already failed (missing-dependency).
> 
> </dd>
> </dl>
> </details>
> 
> + 5 others
> 
> </dd>
> </dl>
> </details>
> 
> <details>
> <summary><strong>🔧 Clippy (1.99.0)</strong></summary>
> <dl>
> <dd>
> 
> Clippy execution failed
> 
> 
> 
> </dd>
> </dl>
> </details>

<!-- end of auto-generated comment: all tool run failures by coderabbit.ai -->
<!-- tips_start -->






<sub>Comment `@coderabbitai help` to get the list of available commands.</sub>

<!-- tips_end -->
