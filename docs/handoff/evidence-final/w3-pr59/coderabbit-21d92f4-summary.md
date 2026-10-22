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
- **Run ID**: `c9eb3ec0-a4f2-4a2e-86dd-5a7e74d6ccc8`

</dd>
</dl>
</details>

<details>
<summary><strong>📥 Commits</strong></summary>
<dl>
<dd>

Reviewing files that changed from the base of the PR and between df2261597ccc577abbf2b96c8682b5bb7b2f3c72 and 21d92f4f73541b4735f618f48987994c1357dc7e.

</dd>
</dl>
</details>

<details>
<summary><strong>📒 Files selected for processing: 4</strong></summary>
<dl>
<dd>

* `apps/web/src/lib/features/events/EventDialog.svelte`
* `apps/web/src/lib/features/events/conflict.ts`
* `apps/web/tests/component/event-conflict-merge.test.ts`
* `apps/web/tests/component/event-save-mine.test.ts`

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
일정 반복 규칙 저장에 기준값 기반 충돌 검사를 추가했습니다. 일정 충돌 시 필드별 병합과 덮어쓰기 확인을 제공합니다. 날짜·시간 표시와 표시 설정, 알림 전체 읽음 처리, 홈 주간 날짜 병합도 변경하고 관련 테스트를 추가했습니다.

**Changes: 이벤트 동시성 처리**

|Layer / File(s)|Summary|
|:---|:---|
|**반복 규칙 충돌 계약과 서버 처리** <br> `contracts/openapi.json`, `crates/server/migrations/0086_event_rule_base.sql`, `crates/server/src/features/recurrence.rs`, `apps/web/src/lib/api/client.ts`, `apps/web/src/lib/features/events/RecurrenceEditor.svelte`, `crates/server/tests/recurrence.rs`, `apps/web/tests/component/event-rule-conflict.test.ts`, `apps/web/tests/component/event-rule-base.test.ts`, `docs/development.md`|요청에 편집 시작 시점의 반복 규칙을 포함합니다. 현재 규칙이 기준 규칙과 다르면 서버가 최신 일정을 담은 409 `rule_conflict`를 반환합니다. 클라이언트는 충돌 응답과 재저장 동작을 처리합니다.|
|**일정 필드 병합과 충돌 UI** <br> `apps/web/src/lib/features/events/conflict.ts`, `apps/web/src/lib/features/events/EventDialog.svelte`, `apps/web/src/lib/features/events/EventForm.svelte`, `apps/web/messages/events/*.json`, `apps/web/tests/component/event-conflict-merge.test.ts`, `apps/web/tests/component/event-save-mine.test.ts`, `apps/web/tests/component/event-dialog-late.test.ts`|기준 입력, 사용자 입력, 최신 서버 값을 필드 단위로 병합합니다. 겹친 필드는 양쪽 값을 표시하고 덮어쓰기를 확인합니다. 요청 중 대화상자 닫기를 막고, 지연된 결과가 다른 일정 화면에 적용되지 않도록 처리합니다.|

**날짜·시간 및 표시 설정**

|Layer / File(s)|Summary|
|:---|:---|
|**날짜·시간 표시와 검증** <br> `apps/web/src/lib/features/events/CalendarScreen.svelte`, `apps/web/src/lib/features/events/labels.ts`, `apps/web/src/lib/features/events/time.ts`, `apps/web/src/lib/features/references/notification-text.ts`, `apps/web/tests/component/date-labels-formatted.test.ts`, `apps/web/tests/component/event-when-hour-cycle.test.ts`, `apps/web/tests/component/calendar-task-create.test.ts`, `apps/web/tests/component/notification-text.test.ts`|달력, 일정, 반복 규칙 및 알림의 날짜에 공통 표시 형식을 사용합니다. 시간 표시에 사용자 설정과 일정 시간대를 적용하며, 알 수 없는 시간대는 `UTC`로 처리합니다.|
|**표시 설정 상태 처리** <br> `apps/web/src/lib/doc-font.ts`, `apps/web/src/lib/features/settings/DisplaySettingsDialog.svelte`, `apps/web/tests/component/display-settings-dialog.test.ts`|현재 적용된 문서 서체를 초기값과 변경 기준으로 사용합니다. 저장된 시간대가 브라우저 목록에 없으면 선택지에 추가하고, 저장 오류는 `errorText`로 변환합니다.|

**홈 날짜 및 알림 처리**

|Layer / File(s)|Summary|
|:---|:---|
|**홈 주간 날짜 병합** <br> `apps/web/src/lib/features/home/HomeDashboard.svelte`, `apps/web/tests/component/home-week-dates.test.ts`|주간 달력 데이터에 각 캘린더의 `dates`를 추가합니다. 해당 날짜가 주간 행에 표시되는지 검증합니다.|
|**알림 전체 읽음 오류 처리** <br> `apps/web/messages/references/*.json`, `apps/web/src/lib/features/references/NotificationsScreen.svelte`, `apps/web/tests/component/notifications-read-all.test.ts`|전체 읽음 요청이 실패하면 목록과 사이드바 갱신을 건너뛰고 오류를 표시합니다. 요청 중 중복 실행을 막고, 실패 후 재시도를 허용합니다.|

<!-- change_assessment_start -->
**Priority:** ⬇️ Low

**Estimated code review effort:** 4 (Complex) | ~45 minutes

<!-- change_assessment_commit:"21d92f4f73541b4735f618f48987994c1357dc7e" -->
**Change:** Bug fix
<!-- change_assessment_end -->

**Possibly related issues**

- fvoci/GroundZero#76: 반복 일정 저장 경로를 다루지만, 이 변경은 `until` 전용 오류 코드나 안내를 추가하지 않습니다.

**Possibly related PRs**

- [fvoci/GroundZero#82](https://github.com/fvoci/GroundZero/pull/82): 업무 충돌 처리에 필드 단위 병합과 덮어쓰기 확인을 구현합니다. 이 PR은 일정 충돌에 유사한 흐름을 적용합니다.
- [fvoci/GroundZero#72](https://github.com/fvoci/GroundZero/pull/72): `EventForm`의 시간 입력 계약은 충돌 병합에서 `when` 값을 비교하는 방식과 직접 연결됩니다.
- [fvoci/GroundZero#80](https://github.com/fvoci/GroundZero/pull/80): 표시 설정 대화상자의 초기화 동작을 변경하며, 현재 적용된 서체를 읽는 변경과 직접 연결됩니다.

</dd>
</dl>
</details>

<!-- walkthrough_end -->
<details>
<summary><strong>Merge Risk:</strong> <strong>⚪ Minimal</strong> · up to <code>21d92</code></summary>
<dl>
<dd>

<!-- final_review_risk_start -->
<!-- final_review_risk_label
**Merge Risk:** **⚪ Minimal** · up to `21d92`
-->
<!-- final_review_risk_coverage:{"sourceCommitId":"21d92f4f73541b4735f618f48987994c1357dc7e","coveredCommitId":"21d92f4f73541b4735f618f48987994c1357dc7e","kind":"reviewed"} -->

Timed-event dates use localized formatting, and the inspected conflict recovery preserves server-only edits. No actionable merge-blocking risk remains after normal checks.
<!-- final_review_risk_end -->

</dd>
</dl>
</details>

<details>
<summary><strong>Security architecture risk:</strong> <strong>🟡 Moderate</strong> · up to <code>21d92</code></summary>
<dl>
<dd>

<!-- architecture_review_start -->
<!-- architecture_review_label
### Security Architecture Review

**Security architecture risk:** **🟡 Moderate** · up to `21d92`
-->

The new recurrence conflict check improves protection against stale rule changes. However, an unchanged-rule save can incorrectly advance an old form’s version, allowing a subsequent save to silently overwrite another editor’s event changes. Existing write permissions limit the affected scope; no privilege escalation was established.

**Retained concerns**
- **Medium · reliability · inferred:** A no-op recurrence save can defeat the event form’s stale-write protection. If editor A opens version N, editor B changes an event field to version N+1, and A saves an unchanged recurrence rule, the new database no-op returns N+1 without writing. EventDialog interprets that increment as its own and advances formVersion while retaining A’s original form input. A subsequent ordinary form save uses N+1 and can overwrite B’s title, description, or schedule without a conflict or merge confirmation. At the PR base, the recurrence save always incremented again to N+2, so this sequence retained A’s stale baseline and produced a conflict. The regression affects shared-state integrity within events the caller already has permission to modify.

<details>
<summary><strong>Security review details</strong></summary>
<dl>
<dd>

**Security Blast Radius**
- _inferred_ — The identified overwrite path is limited by existing event write authority inherited from the owning item. A request affects the selected event’s editable fields; recurrence settings describe the series rather than an individual generated occurrence. The inspected path does not establish access to another tenant’s events or additional privileges.

**Security Findings and Attack Paths**
- _inferred_ — AR-EVENT-001 is an authorized collaboration lost-update path: stale browser fields cross the API boundary with an incorrectly advanced version, and the database accepts them as current. It weakens shared-state integrity and conflict confirmation, but does not demonstrate an authorization bypass. A caller already able to write the event gains no additional write scope.

**Trust Boundaries and Controls**
- _observed_ — The handler binds the authenticated user to the database transaction. Database authorization rejects unavailable events and requires owner or write access, including read-only treatment for archived items. Both recurrence and ordinary event writes recheck authorization after acquiring locks.

**Resilience and Maintainability Implications**
- _observed_ — The dialog captures an opening generation for asynchronous mutations and avoids applying late results to a different opened event. A request counter is decremented in finally, and ordinary close, Escape, and outside-click dismissal are blocked while requests are pending. These controls improve UI failure containment but do not resolve the version-attribution regression.

**Hardening Proposals**
- _proposed_ — Preserve idempotent recurrence retries, but reconcile returned non-rule fields against the form’s original snapshot before advancing its concurrency baseline. A version increment of one should not, by itself, establish ownership of that increment.

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
| Docstring Coverage | ![Warning](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-warning.svg) | Docstring coverage is 52.17% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 46 functions across 20 files. (1 skipped:… | Write docstrings for the functions missing them to satisfy the coverage threshold. |

<details>
<summary><strong><sup><picture><img src="https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-counter-passed-14-v2.svg" width="14" height="14" align="middle" alt="Passed" title="Passed"></picture></sup> Passed checks: 4 passed</strong></summary>
<dl>
<dd>

|         Check name         |                                             Status                                             | Explanation                                                                                  |
| :------------------------ | :--------------------------------------------------------------------------------------------: | :------------------------------------------------------------------------------------------- |
|         Title check        | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | 제목은 반복 규칙 저장 기준 검증과 일정·달력·홈·알림·표시 설정 관련 수정 범위를 정확히 설명합니다. 변경 영역을 여러 개 나열하지만 주된 변경과 직접 관련됩니다. |
|      Description check     | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | 설명은 W21 및 W22~W27의 결함, 수정 내용, 테스트 결과와 미완료 항목을 변경 사항과 직접 연결해 설명합니다.                           |
|     Linked Issues check    | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                     |
| Out of Scope Changes check | ![Passed](https://storage.googleapis.com/coderabbit_public_assets/pre-merge-checks-passed.svg) | Check skipped because no linked issues were found for this pull request.                     |

</dd>
</dl>
</details>

<details>
<summary><strong>Full details: Docstring Coverage</strong></summary>
<dl>
<dd>

**Explanation**

Docstring coverage is 52.17% which is insufficient. The required threshold is 80.00%. Docstring coverage is scoped to functions touched by this diff. Analyzed 46 functions across 20 files. (1 skipped: 1 unsupported.)

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
