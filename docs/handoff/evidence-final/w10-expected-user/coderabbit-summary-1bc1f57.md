<!-- coderabbit-ai-generated:v1 -->

<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/fvoci/GroundZero/pull/60?scope=<redacted>&amp;cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

<!-- review_stack_entry_end -->
<!-- walkthrough_start -->

<details>
<summary><strong>📝 Summary</strong></summary>
<dl>
<dd>

<!-- This is an auto-generated comment: release notes by coderabbit.ai -->

## Summary by CodeRabbit

* **새 기능**
  * 앱 시작 실패 시 번역된 안내와 재시도 버튼을 제공하며, 서버 연결이 복구되면 자동으로 다시 시도합니다.
  * 세션 만료 시 입력 내용을 유지하고 새 탭 로그인을 안내합니다. 다른 계정으로 전환되면 화면을 새로고침합니다.
  * 감사 로그에 보관, 템플릿, 실시간 공동 편집 동작과 상태를 번역해 표시합니다.

* **버그 수정**
  * 계정 전환 후 이전 계정의 문서나 노트가 저장되지 않도록 보호하고, 미저장 편집 내용은 기존 계정의 초안으로 유지합니다.
  * 파일 이름 변경, 멤버 초대, 그룹 생성, 보존 기간 설정 실패 시 관련 입력란에 오류를 표시하고 입력값을 유지합니다.
  * 멤버 변경 충돌 시 목록을 새로 불러오고, 저장된 보기의 중복 삭제 요청을 방지합니다.
  * 요청 크기 초과 및 예상치 못한 오류 안내를 개선했습니다.

* **문서**
  * 계정이 바뀐 경우의 문서 저장 및 실시간 편집 동작에 관한 안내를 갱신했습니다.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
앱 시작·세션 오류, 감사 로그 라벨, 입력 요청 상태를 변경했습니다. 문서 저장·노트 생성·실시간 편집에 기대 사용자 검증을 추가하고, 계정 변경 시 저장을 중단해 기존 계정의 초안을 유지합니다.

**Changes: 앱 시작 및 세션 오류**

|Layer / File(s)|Summary|
|:---|:---|
|**세션 및 앱 오류 처리** <br> `apps/web/src/app/App.svelte`, `apps/web/src/lib/api/*`, `apps/web/src/lib/features/feedback/*`, `apps/web/messages/app/*`, `apps/web/messages/common/*`, `apps/web/tests/component/*session*`, `apps/web/tests/component/*error*`|시작 실패와 세션 만료를 번역된 메시지로 표시합니다. 네트워크 복구 시 자동 재시도하고 수동 재시도 버튼을 제공합니다. 창 포커스 복귀 시 계정을 확인합니다.|
|**감사 로그 라벨** <br> `apps/web/src/lib/features/audit/*`, `apps/web/messages/audit/*`, `apps/web/tests/component/audit-codes.test.ts`|감사 동작과 상태·역할·공개 범위 코드를 표시 이름으로 변환합니다.|

**입력 오류와 요청 상태**

|Layer / File(s)|Summary|
|:---|:---|
|**첨부 파일·멤버 입력** <br> `apps/web/src/lib/features/files/AttachmentsPanel.svelte`, `apps/web/src/lib/features/members/MembersScreen.svelte`, `apps/web/tests/component/file-rename.test.ts`, `apps/web/tests/component/members-*.test.ts`|중복 요청을 차단합니다. 실패한 입력값과 오류를 필드에 유지합니다. 늦은 응답이 새 편집 상태를 덮지 않도록 처리합니다. 409 응답에서는 멤버 상태를 다시 읽습니다.|
|**보존 기간과 저장된 보기** <br> `apps/web/src/lib/features/trash/TrashScreen.svelte`, `apps/web/src/lib/features/tasks/TaskBoard.svelte`, `apps/web/tests/component/trash-retention-field.test.ts`, `apps/web/tests/component/saved-view-delete.test.ts`|필드별 오류를 표시합니다. 이전 저장 요청의 늦은 실패를 무시합니다. 삭제 중인 보기에 대한 중복 요청을 차단합니다.|

**문서 계정 전환 검증**

|Layer / File(s)|Summary|
|:---|:---|
|**서버 계약과 기대 사용자 검증** <br> `contracts/openapi.json`, `crates/server/src/error.rs`, `crates/server/src/features/accounts.rs`, `crates/server/src/features/documents.rs`, `crates/server/src/collab/*`, `crates/server/src/http/mod.rs`, `crates/server/tests/*expected_user*`, `crates/server/tests/collab.rs`|문서 저장과 개인 노트 생성에 `x-fvoci-expected-user` 헤더를 적용합니다. 세션 사용자와 값이 다르면 변경 전에 `409 account_changed`를 반환합니다. 실시간 연결에는 `expected_user`를 적용하고 불일치 시 종료 코드 `4412`를 사용합니다.|
|**웹 편집 계정 고정과 저장 차단** <br> `apps/web/src/lib/api/client.ts`, `apps/web/src/lib/features/documents/*`, `apps/web/src/lib/features/home/HomeDashboard.svelte`, `apps/web/src/lib/features/notes/NotesScreen.svelte`, `apps/web/messages/documents/*`, `apps/web/tests/component/document-save-account*.test.ts`, `apps/web/tests/component/account-switch-unload.test.ts`, `apps/web/tests/component/live-account-changed.test.ts`|편집 시작 계정을 저장·초안·실시간 연결에 전달합니다. 계정 변경이 확인되거나 `account_changed`가 반환되면 전송과 재연결을 중단합니다. 초안은 기존 계정에 유지합니다.|

<!-- change_assessment_start -->
**Priority:** ➖ Normal

**Estimated code review effort:** 5 (Critical) | ~90 minutes

<!-- change_assessment_commit:"1bc1f573fe1c02b9524b8043f231b82b96588746" -->
**Change:** Bug fix
<!-- change_assessment_end -->

**Possibly related issues**

- [fvoci/GroundZero#88](https://github.com/fvoci/GroundZero/issues/88): 계정 전환 감지와 다시 읽기 흐름을 직접 확장하는 변경입니다.

**Possibly related PRs**

- [fvoci/GroundZero#59](https://github.com/fvoci/GroundZero/pull/59): 같은 `DisplaySettingsDialog.svelte` 저장 오류 경로에 `errorText`를 적용합니다.
- [fvoci/GroundZero#64](https://github.com/fvoci/GroundZero/pull/64): 같은 실시간 연결 종료와 미확인 편집 보존 흐름을 확장합니다.
- [fvoci/GroundZero#57](https://github.com/fvoci/GroundZero/pull/57): 계정별 문서 초안 보존 동작을 확장합니다.

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong>🔵 Low</strong> · up to <code>1bc1f</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **🔵 Low** · up to `1bc1f`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"1bc1f573fe1c02b9524b8043f231b82b96588746","coveredCommitId":"1bc1f573fe1c02b9524b8043f231b82b96588746","kind":"reviewed"} -->

An upload that expires during an account switch could restart under the new account. Address that edge case or accept it as a bounded follow-up before merging.
<!-- final_review_risk_end -->

</dd>
</dl>
</details>

<details>
<summary><strong>Security architecture risk:</strong> <strong>🟡 Moderate</strong> · up to <code>1bc1f</code></summary>
<dl>
<dd>

<!-- architecture_review_start -->
<!-- architecture_review_label
### Security Architecture Review

**Security architecture risk:** **🟡 Moderate** · up to `1bc1f`
-->

Binding edits to their starting account strengthens isolation. However, automatic account-switch recovery can discard unconfirmed live edits when local draft storage fails. Older clients also become incompatible with the newly required identity fields.

**Retained concerns**
- **Medium · reliability · inferred:** The new live account-mismatch transition reloads the page without confirming durable preservation of pending edits. On close code 4412, keepIfDirty attempts storage, disconnects and invokes switchAccount unconditionally. storeDraft returns false when storage is unavailable or full, but that result is discarded. If the latest unconfirmed edits have no durable copy, the reload loses their in-memory recovery state. Best-effort draft storage predates this PR; the automatic mismatch-to-page-reload transition is new. Successful-storage coverage and the ordinary editor's unload safeguard do not protect this live path.

<details>
<summary><strong>Security review details</strong></summary>
<dl>
<dd>

**Security Blast Radius**
- _observed_ — The new enforcement covers document saves, personal-note creation and collaboration connections, including documents shared across workspace members. Existing session-based document permissions still determine access; the expected-user value cannot enlarge the account's accessible document scope. Other write endpoints are explicitly outside this contract.

**Trust Boundaries and Controls**
- _observed_ — Client-controlled expected identity is checked against authenticated session identity, never adopted as authority. HTTP mismatches terminate before handler transactions. Collaboration also retains its origin check and performs identity comparison before document access. Inspected integration-test assertions cover mismatch without mutation, malformed or duplicate headers, and missing authentication; those tests were not executed during this review.

**Resilience and Maintainability Implications**
- _inferred_ — Stopping a mismatched live connection preserves the server-side account boundary, but recovery is not failure-atomic with local persistence. The new reload proceeds even when the original account's latest pending edits could not be stored. The supplied live account-change test covers successful draft preservation, not persistence failure during that transition.

**Hardening Proposals**
- _proposed_ — Treat durable original-account draft preservation as a prerequisite for automatic live-editor teardown. If preservation fails, retain a disconnected recovery state without allowing writes under the replacement account.

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
| Docstring Coverage | ![Warning](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-warning.svg) | Docstring coverage is 63.89% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 72 functions across 29 files. (6 skipped:… | Write docstrings for the functions missing them to satisfy the coverage threshold. |

<details>
<summary><strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" width="14" height="14" align="middle" alt="Passed" title="Passed"></picture></sup> Passed checks: 4 passed</strong></summary>
<dl>
<dd>

|         Check name         |                                             Status                                             | Explanation                                                                                                         |
| :------------------------ | :--------------------------------------------------------------------------------------------: | :------------------------------------------------------------------------------------------------------------------ |
|         Title check        | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | 제목은 앱 껍데기, 멤버, 파일, 휴지통, 감사 기록의 주요 웹 수정 사항을 정확히 설명합니다. 기대 사용자 검증과 실시간 편집 변경까지 모두 포함하지는 않지만, 변경사항과 직접 관련된 구체적인 제목입니다. |
|      Description check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | 설명은 중복 요청, 세션 만료, 계정 전환, 오류 표시, 접근성, 기대 사용자 검증 및 실시간 편집 변경을 구체적으로 설명합니다. 변경사항과 직접 관련되며 검증 결과도 포함합니다.                |
|     Linked Issues check    | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                            |
| Out of Scope Changes check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                            |

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Docstring Coverage</strong></summary>
<dl>
<dd>

**Explanation**

Docstring coverage is 63.89% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 72 functions across 29 files. (6 skipped: 6 unsupported.)

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
