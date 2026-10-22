<!-- coderabbit-ai-generated:v1 -->

<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/fvoci/GroundZero/pull/60?scope=<redacted>&amp;cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

<!-- review_stack_entry_end -->
<!-- This is an auto-generated comment: review paused by coderabbit.ai -->

> [!NOTE]
> ## Reviews paused
> 
> It looks like this branch is under active development. To avoid overwhelming you with review comments due to an influx of new commits, CodeRabbit has automatically paused this review. You can configure this behavior by changing the `reviews.auto_review.auto_pause_after_reviewed_commits` setting.
> 
> - Last reviewed commit: a811f92500200eb61c0831baf3ac72e3b9c7a6e4
> 
> Use the following commands to manage reviews:
> - `@coderabbitai resume` to resume automatic reviews.
> - `@coderabbitai review` to trigger a single review.
> 
> Use the checkboxes below for quick actions:
> - [ ] <!-- {"checkboxId":"7f6cc2e2-2e4e-497a-8c31-c9e4573e93d1"} --> ▶️ Resume reviews
> - [ ] <!-- {"checkboxId":"e9bb8d72-00e8-4f67-9cb2-caf3b22574fe"} --> 🔍 Trigger review

<!-- end of auto-generated comment: review paused by coderabbit.ai -->
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
- **Run ID**: `e01f196b-a86d-4e9c-ad52-8e36059b618c`

</dd>
</dl>
</details>

<details>
<summary><strong>📥 Commits</strong></summary>
<dl>
<dd>

Reviewing files that changed from the base of the PR and between 8e420369a4993cc611550cb63bd7fa06bac64e7a and a811f92500200eb61c0831baf3ac72e3b9c7a6e4.

</dd>
</dl>
</details>

<details>
<summary><strong>📒 Files selected for processing: 4</strong></summary>
<dl>
<dd>

* `apps/web/src/lib/features/documents/LiveDocumentEditor.svelte`
* `apps/web/tests/component/live-title-before-sync.test.ts`
* `apps/web/tests/component/stubs/SilentEditorStub.svelte`
* `docs/development.md`

</dd>
</dl>
</details>

**Included review availability:** This review used your included allowance. Your plan provides up to 10 included reviews per hour; 4 remain after this review.

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
  * 앱 시작에 실패하면 번역된 안내와 재시도 버튼을 표시하며, 서버 연결이 복구되면 자동으로 다시 시도합니다.
  * 세션이 만료되면 입력 내용을 유지하고 새 탭 로그인을 안내합니다. 다른 계정으로 로그인하면 계정 전환을 알리고 화면을 새로고침합니다.
  * 감사 로그에 보관, 템플릿, 실시간 공동 편집 동작과 상태를 번역해 표시합니다.

* **버그 수정**
  * 계정 전환 후 이전 계정의 문서, 노트, 업로드가 잘못 처리되지 않도록 보호합니다. 미저장 편집 내용은 가능한 경우 기존 계정의 초안으로 보관합니다.
  * 파일 이름 변경, 멤버 초대, 그룹 생성, 보존 기간 설정 실패 시 입력란에 오류를 표시하고 입력값을 유지합니다.
  * 멤버 변경 충돌 시 목록을 다시 불러오며, 저장된 보기의 중복 삭제 요청을 방지합니다.
  * 요청 크기 초과 및 예상치 못한 오류 안내를 개선했습니다.

* **문서**
  * 계정이 바뀐 경우의 문서 저장 및 실시간 편집 동작에 관한 안내를 갱신했습니다.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
앱 오류·세션 만료·감사 라벨과 입력 오류 처리를 보강했습니다. 문서 저장·노트 생성·업로드·실시간 편집에는 편집 시작 계정 검증을 추가했습니다. 계정 변경 시 저장과 재연결을 중단하고 기존 계정의 초안을 보존합니다.

**Changes: 웹 앱 오류와 입력 상태**

|Layer / File(s)|Summary|
|:---|:---|
|**앱·세션 및 감사 로그** <br> `apps/web/src/app/App.svelte`, `apps/web/src/lib/api/*`, `apps/web/src/lib/features/feedback/*`, `apps/web/src/lib/features/audit/*`, `apps/web/messages/*`|시작 오류와 세션 만료를 번역된 메시지로 표시합니다. 네트워크 복구 시 재시도합니다. 감사 코드와 상태·역할·공개 범위를 표시 이름으로 변환합니다.|
|**입력 오류와 요청 상태** <br> `apps/web/src/lib/features/files/AttachmentsPanel.svelte`, `apps/web/src/lib/features/members/MembersScreen.svelte`, `apps/web/src/lib/features/trash/TrashScreen.svelte`, `apps/web/src/lib/features/tasks/TaskBoard.svelte`|실패한 입력값과 오류를 필드에 유지합니다. 중복 요청을 차단합니다. 늦은 응답이 최신 입력 상태를 덮지 않도록 처리합니다. 충돌 시 멤버 상태를 다시 읽습니다.|

**문서 계정 전환 검증**

|Layer / File(s)|Summary|
|:---|:---|
|**서버 계약과 검증** <br> `contracts/openapi.json`, `crates/server/src/error.rs`, `crates/server/src/features/accounts.rs`, `crates/server/src/features/documents.rs`, `crates/server/src/features/files.rs`, `crates/server/src/collab/*`, `crates/server/src/http/mod.rs`|문서 저장·개인 노트 생성·업로드 시작에 `x-fvoci-expected-user` 헤더를 적용합니다. 값이 세션 사용자와 다르면 변경 전에 `409 account_changed`를 반환합니다. 실시간 연결은 `expected_user`를 확인하고 불일치 시 `4412`로 종료합니다.|
|**웹 편집 계정 고정** <br> `apps/web/src/lib/api/client.ts`, `apps/web/src/lib/features/documents/*`, `apps/web/src/lib/features/files/upload.ts`, `apps/web/messages/documents/*`|편집 시작 계정을 저장·초안·업로드·실시간 연결에 전달합니다. 계정 변경이 확인되거나 `account_changed`가 반환되면 저장과 재시도를 중단합니다. 초안은 기존 계정에 유지합니다.|

<!-- change_assessment_start -->
**Priority:** ➖ Normal

**Estimated code review effort:** 5 (Critical) | ~90 minutes

<!-- change_assessment_commit:"a811f92500200eb61c0831baf3ac72e3b9c7a6e4" -->
**Change:** Bug fix · **Severity of issue fixed:** Medium
<!-- change_assessment_end -->

**Possibly related issues**

- [fvoci/GroundZero#88](https://github.com/fvoci/GroundZero/issues/88): 계정 전환 감지와 다시 읽기 실패 처리의 후속 요구로, 같은 계정 전환 흐름과 직접 연결됩니다.

**Possibly related PRs**

- [fvoci/GroundZero#59](https://github.com/fvoci/GroundZero/pull/59): `DisplaySettingsDialog.svelte`의 저장 오류를 `errorText`로 현지화하는 같은 경로를 변경합니다.
- [fvoci/GroundZero#64](https://github.com/fvoci/GroundZero/pull/64): `LiveDocumentEditor`의 실시간 종료와 미저장 편집 보존 흐름을 확장합니다.
- [fvoci/GroundZero#57](https://github.com/fvoci/GroundZero/pull/57): `DocumentEditor`의 계정별 초안 보존 흐름을 확장합니다.

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong>⚪ Minimal</strong> · up to <code>a811f</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **⚪ Minimal** · up to `a811f`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"a811f92500200eb61c0831baf3ac72e3b9c7a6e4","coveredCommitId":"a811f92500200eb61c0831baf3ac72e3b9c7a6e4","kind":"reviewed"} -->

계정 전환 시 초안을 저장하지 못하면 이탈 경고가 유지되고, 첫 동기화 전 제목 변경도 초안에 보존됩니다. 확인된 미해결 병합 위험은 없습니다.
<!-- final_review_risk_end -->

</dd>
</dl>
</details>

<details>
<summary><strong>Security architecture risk:</strong> <strong>🔵 Low</strong> · up to <code>a811f</code></summary>
<dl>
<dd>

<!-- architecture_review_start -->
<!-- architecture_review_label
### Security Architecture Review

**Security architecture risk:** **🔵 Low** · up to `a811f`
-->

The change strengthens account isolation without granting additional permissions. Covered requests reject a changed account before protected work begins. Remaining uncertainty concerns older-client compatibility, deployment ordering and real-browser recovery behavior.

**Retained concerns**
No architecture-level concerns identified.

<details>
<summary><strong>Security review details</strong></summary>
<dl>
<dd>

**Security Blast Radius**
- _inferred_ — The addressed exposure is stale browser work crossing an account switch: document edits, recovered personal notes, upload starts and live reconnects. Effective access remains bounded by the authenticated session and existing resource permissions; supplying another user's UUID does not grant access to that user's resources.

**Trust Boundaries and Controls**
- _observed_ — The caller-controlled expected identity crosses into the server only as a consistency assertion. ExpectedUser first resolves the cookie session, compares the assertion, and returns the session-derived user. Document write authorization still occurs separately, and live connections retain the origin check before identity comparison.

**Resilience and Maintainability Implications**
- _observed_ — On live account mismatch, the provider disconnects and reload occurs only when there is no pending work or draft preservation succeeds. Failed preservation leaves an explicit warning and the current screen. The regular editor's account-switch unload path likewise suppresses its warning only after a successful storage write.

**Hardening Proposals**
- _proposed_ — Validate mixed-version deployment and rollback with already-open browser tabs and external callers. Preserve fail-closed identity enforcement while providing a clear refresh or recovery route for legacy callers; include real-browser account-switch tests with unavailable storage and interrupted reconnects.

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
| Docstring Coverage | ![Warning](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-warning.svg) | Docstring coverage is 63.64% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 77 functions across 33 files. (3 skipped:… | Write docstrings for the functions missing them to satisfy the coverage threshold. |

<details>
<summary><strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" width="14" height="14" align="middle" alt="Passed" title="Passed"></picture></sup> Passed checks: 4 passed</strong></summary>
<dl>
<dd>

|         Check name         |                                             Status                                             | Explanation                                                                                                                |
| :------------------------ | :--------------------------------------------------------------------------------------------: | :------------------------------------------------------------------------------------------------------------------------- |
|         Title check        | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | 제목은 저장된 보기, 멤버, 파일, 휴지통, 감사 기록 관련 수정 사항을 명확히 요약합니다. 기대 사용자 검증과 계정 전환 처리라는 추가 주요 범위는 제목에 포함되지 않았지만, 제목은 변경 사항과 관련되고 구체적입니다. |
|      Description check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | 설명은 감사 결함 수정, 세션 만료, 계정 전환, 기대 사용자 검증, 실시간 편집 처리 및 관련 시험을 구체적으로 설명합니다. 변경 사항과 직접 관련됩니다.                                    |
|     Linked Issues check    | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                                   |
| Out of Scope Changes check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                                   |

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Docstring Coverage</strong></summary>
<dl>
<dd>

**Explanation**

Docstring coverage is 63.64% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 77 functions across 33 files. (3 skipped: 3 unsupported.)

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
