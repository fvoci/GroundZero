<!-- coderabbit-ai-generated:v1 -->

<!-- This is an auto-generated comment: summarize by coderabbit.ai -->
<!-- review_stack_entry_start -->

<a href="https://app.coderabbit.ai/change-stack/fvoci/GroundZero/pull/59?scope=<redacted>&amp;cs_source=review_comment"><picture><source media="(prefers-color-scheme: dark)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui-dark.svg?v=3"><source media="(prefers-color-scheme: light)" srcset="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3"><img src="https://storage.googleapis.com/coderabbit_public_assets/review-stack-in-coderabbit-ui.svg?v=3" alt="Review in Change Stack →" height="32"></picture></a>

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
- **Run ID**: `c50da65d-5037-4a2a-808b-b670047198ab`

</dd>
</dl>
</details>

<details>
<summary><strong>📥 Commits</strong></summary>
<dl>
<dd>

Reviewing files that changed from the base of the PR and between dbd18619c7f20757df432103a09e87e3caeaf2ac and df2261597ccc577abbf2b96c8682b5bb7b2f3c72.

</dd>
</dl>
</details>

<details>
<summary><strong>📒 Files selected for processing: 7</strong></summary>
<dl>
<dd>

* `apps/web/messages/events/en.json`
* `apps/web/messages/events/ko.json`
* `apps/web/src/lib/features/events/EventDialog.svelte`
* `apps/web/src/lib/features/events/EventForm.svelte`
* `apps/web/src/lib/features/events/conflict.ts`
* `apps/web/tests/component/event-conflict-merge.test.ts`
* `apps/web/tests/component/event-save-mine.test.ts`

</dd>
</dl>
</details>

<details>
<summary><strong>🚧 Files skipped from review as they are similar to previous changes: 2</strong></summary>
<dl>
<dd>

* apps/web/messages/events/en.json
* apps/web/messages/events/ko.json

</dd>
</dl>
</details>

**Included review availability:** This review used your included allowance. Your plan provides up to 10 included reviews per hour; 8 remain after this review.

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

* **개선 사항**
  * 달력, 일정, 반복 종료일 및 알림 날짜가 현재 언어와 지역 형식으로 표시됩니다.
  * 시간 표시가 선택한 12시간제·24시간제 설정을 따릅니다.
  * 홈 주간 달력에 개인 일정과 마감일 등 추가 날짜 정보가 표시됩니다.
  * 일정 저장·삭제 중 지연된 결과가 현재 열린 일정에 잘못 반영되지 않습니다.
  * 반복 일정 및 일정 내용 충돌을 확인하고 원하는 변경 사항을 안전하게 병합·저장할 수 있습니다.
  * 알림 일괄 읽음 처리 실패 시 목록을 유지하고 오류 안내와 재시도를 제공합니다.
  * 표시 설정에서 저장된 시간대와 현재 적용된 문서 글꼴이 올바르게 유지됩니다.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
날짜·시간 표시를 현지화하고 이벤트 저장의 동시 변경 충돌 처리를 추가했습니다. 알림 전체 읽음 실패, 홈 주간 날짜, 표시 설정 상태 처리도 변경하고 관련 테스트를 추가했습니다.

**Changes: 날짜 및 시간 표시**

|Layer / File(s)|Summary|
|:---|:---|
|**날짜 표시와 검증** <br> `apps/web/src/lib/features/events/...`, `apps/web/src/lib/features/references/notification-text.ts`, `apps/web/tests/component/date-labels-formatted.test.ts`|달력, 일정, 반복 규칙 및 알림의 날짜를 `formatDay`로 표시합니다. 날짜 라벨과 접근성 텍스트를 검증합니다.|
|**시간대와 시간 주기 표시** <br> `apps/web/src/lib/features/events/labels.ts`, `apps/web/src/lib/features/events/time.ts`, `apps/web/tests/component/event-when-hour-cycle.test.ts`|시간 표시가 표시 설정과 일정 시간대를 따릅니다. 알 수 없는 시간대는 `UTC`로 처리합니다.|

**이벤트 동시성 처리**

|Layer / File(s)|Summary|
|:---|:---|
|**반복 규칙 충돌 계약과 서버 처리** <br> `contracts/openapi.json`, `crates/server/migrations/0086_event_rule_base.sql`, `crates/server/src/features/recurrence.rs`, `crates/server/src/lib/api/client.ts`, `crates/server/tests/recurrence.rs`|반복 규칙 저장 요청에 `base`를 추가합니다. 규칙이 변경되면 최신 이벤트를 포함한 409 `rule_conflict`를 반환합니다.|
|**이벤트 필드 병합과 충돌 UI** <br> `apps/web/src/lib/features/events/EventDialog.svelte`, `apps/web/src/lib/features/events/conflict.ts`, `apps/web/src/lib/features/events/EventForm.svelte`|서버 최신 일정과 사용자 입력을 병합합니다. 동일 필드의 동시 변경은 서버 값과 사용자 값을 표시하고 덮어쓰기 여부를 확인합니다.|
|**이벤트 동시성 검증** <br> `apps/web/tests/component/event-*.test.ts`|반복 규칙 충돌, 필드 병합, 지연 응답, 재저장 및 대화상자 상태를 검증합니다.|

**지원 기능 동작**

|Layer / File(s)|Summary|
|:---|:---|
|**홈 주간 날짜 병합** <br> `apps/web/src/lib/features/home/HomeDashboard.svelte`, `apps/web/tests/component/home-week-dates.test.ts`|주간 달력 데이터에 각 캘린더의 `dates`를 포함합니다. 날짜가 해당 주간 행에 표시되는지 검증합니다.|
|**알림 전체 읽음 오류 처리** <br> `apps/web/messages/references/*.json`, `apps/web/src/lib/features/references/NotificationsScreen.svelte`, `apps/web/tests/component/notifications-read-all.test.ts`|전체 읽음 요청 실패 시 현지화된 오류를 표시합니다. 실패하면 목록과 사이드바를 갱신하지 않습니다. 중복 요청을 차단합니다.|
|**표시 설정 상태 처리** <br> `apps/web/src/lib/doc-font.ts`, `apps/web/src/lib/features/settings/DisplaySettingsDialog.svelte`, `apps/web/tests/component/display-settings-dialog.test.ts`|현재 적용된 문서 서체를 초기값과 변경 기준으로 사용합니다. 지원 목록에 없는 시간대를 선택지에 포함하고 저장 오류를 `errorText`로 변환합니다.|

<!-- change_assessment_start -->
**Priority:** ➖ Normal

**Estimated code review effort:** 4 (Complex) | ~45 minutes

<!-- change_assessment_commit:"df2261597ccc577abbf2b96c8682b5bb7b2f3c72" -->
**Change:** Bug fix
<!-- change_assessment_end -->

**Possibly related issues**

- fvoci/GroundZero#76: 같은 반복 일정 저장 경로를 다루지만, 이 변경에는 `until` 전용 오류 코드와 안내가 포함되지 않습니다.

**Possibly related PRs**

- [fvoci/GroundZero#72](https://github.com/fvoci/GroundZero/pull/72): `EventForm`의 시간 입력 계약이 이벤트 충돌 병합의 `when` 비교 결과에 직접 연결됩니다.
- [fvoci/GroundZero#80](https://github.com/fvoci/GroundZero/pull/80): `DisplaySettingsDialog.svelte`의 초기화 effect를 보완하며 적용된 서체 초기화와 직접 연결됩니다.

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong>⚪ Minimal</strong> · up to <code>df226</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **⚪ Minimal** · up to `df226`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"df2261597ccc577abbf2b96c8682b5bb7b2f3c72","coveredCommitId":"df2261597ccc577abbf2b96c8682b5bb7b2f3c72","kind":"reviewed"} -->

Timed events now show localized dates, and the earlier display concern is fixed. No actionable merge-blocking risk remains; a real-browser check of the conflict dialog remains as normal pre-merge validation.
<!-- final_review_risk_end -->

</dd>
</dl>
</details>

<details>
<summary><strong>Security architecture risk:</strong> <strong>🔵 Low</strong> · up to <code>df226</code></summary>
<dl>
<dd>

<!-- architecture_review_start -->
<!-- architecture_review_label
### Security Architecture Review

**Security architecture risk:** **🔵 Low** · up to `df226`
-->

The inspected changes strengthen protection against stale event edits while preserving existing write permissions. The database and server changes require coordinated deployment. No introduced security vulnerability was established, but deployment and broader change coverage remain incomplete.

**Retained concerns**
No architecture-level concerns identified.

<details>
<summary><strong>Security review details</strong></summary>
<dl>
<dd>

**Security Blast Radius**
- _inferred_ — In the inspected recurrence path, caller-controlled event IDs and rule/base values reach an authenticated, user-scoped transaction. Mutation remains bounded by write access to the event's parent item. The changed merge flow does not submit event identity, ownership or access metadata as editable fields; no broader mutation authority was established.

**Trust Boundaries and Controls**
- _observed_ — Existing event-access enforcement treats deleted events as absent and derives access from the parent item, downgrading archived items to read access. Consequently, a subsequent delete request started after deletion commits is rejected by the existing write-access helper rather than creating another trash batch. These controls predate the PR.

**Resilience and Maintainability Implications**
- _observed_ — The dialog uses a displayed-event generation to prevent late save, recurrence and delete results from replacing or closing another event's view. Pending requests block user-driven dialog closure, and pending accounting is released in a finally block. These changes improve containment of interrupted or overlapping UI operations without replacing server authorization.

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
| Docstring Coverage | ![Warning](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-warning.svg) | Docstring coverage is 52.27% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 44 functions across 20 files. (4 skipped:… | Write docstrings for the functions missing them to satisfy the coverage threshold. |

<details>
<summary><strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" width="14" height="14" align="middle" alt="Passed" title="Passed"></picture></sup> Passed checks: 4 passed</strong></summary>
<dl>
<dd>

|         Check name         |                                             Status                                             | Explanation                                                                                              |
| :------------------------ | :--------------------------------------------------------------------------------------------: | :------------------------------------------------------------------------------------------------------- |
|         Title check        | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | 제목은 일정·달력·홈·알림·표시 설정 수정과 반복 규칙 기준 검증이라는 변경 범위를 정확히 설명합니다. 감사 항목과 Draft 상태가 포함되어 다소 길지만, 주요 변경과 직접 관련됩니다. |
|      Description check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | 설명은 W21 및 W22~W27의 결함, 수정 내용, 시험 결과와 남은 검증 범위를 변경 사항에 맞게 설명합니다.                                          |
|     Linked Issues check    | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                 |
| Out of Scope Changes check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                                 |

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Docstring Coverage</strong></summary>
<dl>
<dd>

**Explanation**

Docstring coverage is 52.27% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 44 functions across 20 files. (4 skipped: 4 unsupported.)

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
<!-- tips_start -->






<sub>Comment `@coderabbitai help` to get the list of available commands.</sub>

<!-- tips_end -->
