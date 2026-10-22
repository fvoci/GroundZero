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
  * 앱 시작에 실패하면 번역된 안내와 재시도 버튼을 표시하며, 서버 연결이 복구되면 자동으로 다시 시도합니다.
  * 세션이 만료되면 입력 내용을 유지하고 새 탭 로그인을 안내합니다. 다른 계정으로 로그인하면 계정 전환을 알리고 화면을 새로고침합니다.
  * 감사 로그에 보관, 템플릿, 실시간 공동 편집 동작과 상태를 번역해 표시합니다.

* **버그 수정**
  * 계정 전환 후 이전 계정의 문서, 노트, 업로드가 잘못 처리되지 않도록 보호하고, 미저장 편집 내용은 가능한 경우 기존 계정의 초안으로 보관합니다.
  * 파일 이름 변경, 멤버 초대, 그룹 생성, 보존 기간 설정 실패 시 입력란에 오류를 표시하고 입력값을 유지합니다.
  * 멤버 변경 충돌 시 목록을 새로 불러오고, 저장된 보기의 중복 삭제 요청을 방지합니다.
  * 요청 크기 초과 및 예상치 못한 오류 안내를 개선했습니다.

* **문서**
  * 계정이 바뀐 경우의 문서 저장 및 실시간 편집 동작에 관한 안내를 갱신했습니다.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
앱 시작·세션 오류와 감사 라벨을 갱신했습니다. 입력 오류와 중복 요청을 처리했습니다. 문서 저장·노트 생성·업로드·실시간 편집에 기대 사용자 검증을 적용하고, 계정 변경 시 전송과 재연결을 중단합니다.

**Changes: 웹 앱 오류와 입력 상태**

|Layer / File(s)|Summary|
|:---|:---|
|**앱·세션 및 감사 로그** <br> `apps/web/src/app/App.svelte`, `apps/web/src/lib/api/*`, `apps/web/src/lib/features/feedback/*`, `apps/web/src/lib/features/audit/*`, `apps/web/messages/*`|시작 오류와 세션 만료를 번역된 메시지로 표시합니다. 네트워크 복구 시 재시도합니다. 감사 코드와 상태·역할·공개 범위를 표시 이름으로 변환합니다.|
|**입력 오류와 요청 상태** <br> `apps/web/src/lib/features/files/AttachmentsPanel.svelte`, `apps/web/src/lib/features/members/MembersScreen.svelte`, `apps/web/src/lib/features/trash/TrashScreen.svelte`, `apps/web/src/lib/features/tasks/TaskBoard.svelte`|실패한 입력값과 오류를 필드에 유지합니다. 중복 요청을 차단합니다. 늦은 응답이 최신 입력 상태를 덮지 않도록 처리합니다. 충돌 시 멤버 상태를 다시 읽습니다.|

**문서 계정 전환 검증**

|Layer / File(s)|Summary|
|:---|:---|
|**서버 계약과 검증** <br> `contracts/openapi.json`, `crates/server/src/error.rs`, `crates/server/src/features/accounts.rs`, `crates/server/src/features/documents.rs`, `crates/server/src/features/files.rs`, `crates/server/src/collab/*`, `crates/server/src/http/mod.rs`|문서 저장·개인 노트 생성·업로드 시작에 `x-fvoci-expected-user` 헤더를 적용합니다. 값이 세션 사용자와 다르면 변경 전에 `409 account_changed`를 반환합니다. 실시간 연결은 `expected_user`를 확인하고 불일치 시 종료 코드 `4412`를 사용합니다.|
|**웹 편집 계정 고정** <br> `apps/web/src/lib/api/client.ts`, `apps/web/src/lib/features/documents/*`, `apps/web/src/lib/features/files/upload.ts`, `apps/web/messages/documents/*`|편집 시작 계정을 저장·초안·업로드·실시간 연결에 전달합니다. 계정 변경이 확인되거나 `account_changed`가 반환되면 저장과 재시도를 중단합니다. 초안은 기존 계정에 유지합니다.|

<!-- change_assessment_start -->
**Priority:** ➖ Normal

**Estimated code review effort:** 5 (Critical) | ~90 minutes

<!-- change_assessment_commit:"8e420369a4993cc611550cb63bd7fa06bac64e7a" -->
**Change:** Bug fix
<!-- change_assessment_end -->

**Possibly related issues**

- [fvoci/GroundZero#88](https://github.com/fvoci/GroundZero/issues/88): 계정 전환 감지와 다시 읽기 실패 처리의 후속 요구로, 같은 계정 전환 흐름과 직접 연결됩니다.

**Possibly related PRs**

- [fvoci/GroundZero#59](https://github.com/fvoci/GroundZero/pull/59): `DisplaySettingsDialog.svelte`의 같은 저장 오류 경로를 `errorText`로 변경합니다.
- [fvoci/GroundZero#64](https://github.com/fvoci/GroundZero/pull/64): 같은 실시간 종료와 미확인 편집 보존 흐름을 확장합니다.
- [fvoci/GroundZero#57](https://github.com/fvoci/GroundZero/pull/57): 계정별 문서 초안 보존 동작을 확장합니다.

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong>🔵 Low</strong> · up to <code>8e420</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **🔵 Low** · up to `8e420`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"8e420369a4993cc611550cb63bd7fa06bac64e7a","coveredCommitId":"8e420369a4993cc611550cb63bd7fa06bac64e7a","kind":"reviewed"} -->

When an account change is rejected before the first body sync, a title-only edit may not be saved as a draft and switching or leaving can be blocked. The edit remains on screen and the trigger is narrow, so risk is low; using the initial body as a draft fallback is recommended before merging.
<!-- final_review_risk_end -->

</dd>
</dl>
</details>

<details>
<summary><strong>Security architecture risk:</strong> <strong>🔵 Low</strong> · up to <code>8e420</code></summary>
<dl>
<dd>

<!-- architecture_review_start -->
<!-- architecture_review_label
### Security Architecture Review

**Security architecture risk:** **🔵 Low** · up to `8e420`
-->

The new checks reduce the risk of saving one account’s edits under another account without transferring authorization to the client. No introduced authorization bypass was verified. Mixed-version deployment and browser-level draft recovery still need confirmation.

**Retained concerns**
No architecture-level concerns identified.

<details>
<summary><strong>Security review details</strong></summary>
<dl>
<dd>

**Security Blast Radius**
- _observed_ — The new consistency gate covers document saves, personal-note creation, upload initiation and restart, and live connection establishment. It is not a global mutation policy. Upload parts and completion instead retain persisted upload-owner checks.

**Security Findings and Attack Paths**
- _inferred_ — No assertion-based privilege gain was verified in the inspected paths. Supplying another user’s UUID fails the identity comparison; supplying the authenticated user’s UUID does not bypass ordinary access checks. The change addresses stale-account attribution rather than authorizing a client-selected identity.

**Trust Boundaries and Controls**
- _observed_ — The browser’s opening identity crosses the trust boundary only as a consistency assertion. Server-derived session identity remains authoritative, and mismatched live connections are closed before joining a room or inheriting document privileges.

**Resilience and Maintainability Implications**
- _observed_ — Already-open live sockets retain their original authenticated user and session token, with periodic session and document-access rechecks. Continuing under a still-valid original session is documented pre-existing behavior, not a newly introduced switch to another account’s authority.

**Hardening Proposals**
- _proposed_ — Document and validate a version-aligned rollout and rollback procedure that handles existing browser tabs and avoids treating older server instances as enforcing the new consistency guarantee. Preserve fail-closed assertion checks rather than adding an implicit legacy bypass.

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
| Docstring Coverage | ![Warning](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-warning.svg) | Docstring coverage is 63.64% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 77 functions across 32 files. (2 skipped:… | Write docstrings for the functions missing them to satisfy the coverage threshold. |

<details>
<summary><strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" width="14" height="14" align="middle" alt="Passed" title="Passed"></picture></sup> Passed checks: 4 passed</strong></summary>
<dl>
<dd>

|         Check name         |                                             Status                                             | Explanation                                                                                                                  |
| :------------------------ | :--------------------------------------------------------------------------------------------: | :--------------------------------------------------------------------------------------------------------------------------- |
|         Title check        | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | 제목은 앱 껍데기, 멤버, 파일, 휴지통, 감사 기록 관련 변경을 정확히 나타냅니다. 다만 기대 사용자 검증과 계정 전환 처리 같은 큰 변경은 제목에 포함하지 않지만, 제목이 변경 내용과 직접 관련되므로 기준을 충족합니다. |
|      Description check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | 설명은 감사 항목 수정, 세션 만료, 계정 전환, 기대 사용자 계약, 실시간 편집, 테스트 범위를 구체적으로 설명합니다. 변경 내용과 직접 관련되며 충분한 검증 정보도 포함합니다.                         |
|     Linked Issues check    | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                                     |
| Out of Scope Changes check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                                     |

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Docstring Coverage</strong></summary>
<dl>
<dd>

**Explanation**

Docstring coverage is 63.64% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 77 functions across 32 files. (2 skipped: 2 unsupported.)

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
> ESLint install timed out. The project may have too many dependencies for the sandbox.
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
