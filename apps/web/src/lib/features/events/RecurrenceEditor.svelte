<script lang="ts">
  // 반복 규칙 입력(업무·일정 공통, D-31). 단순한 규칙만: 매일·매주(요일)·매달·매년, 간격, 끝(없음·날짜·횟수).
  // 없는 날(31일·2월 29일)은 건너뛴다. 저장하면 이미 만든 회차는 바뀌지 않는다.
  import type { Frequency, RecurrenceRule } from '$lib/api/client';
  import { onDestroy, untrack } from 'svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import {
    parseRuleDraft,
    type RuleDraft,
    type RuleRawDraft,
    type RuleComparison,
  } from './input-drafts';
  import { ApiError, FREQUENCIES } from '$lib/api/client';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { errorText, untilBeforeStart } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import FieldConflict from '$lib/features/common/FieldConflict.svelte';
  import {
    fieldDifferences,
    initialFieldChoices,
    selectedFields,
  } from '$lib/features/common/field-conflict';
  import { ruleConflictFields, sameRule, type RuleValue } from './conflict';
  import { describeRule, ISO_WEEKDAYS, weekdayLabel } from './labels';

  interface Props {
    idPrefix: string;
    rule: RecurrenceRule | null;
    canEdit: boolean;
    /** 이 반복의 시간대(업무는 "오늘"의 기준, 일정은 일정 시간대). */
    zoneNote: string;
    onsave: (rule: RecurrenceRule | null, baseRule?: RecurrenceRule | null) => Promise<void>;
    draftName?: string;
    onunsaved?: (value: boolean) => void;
    ondraftkept?: (value: boolean) => void;
    onsaving?: (value: boolean) => void;
    untilError?: string | undefined;
    /** 부모가 저장하는 동안 잠근다(반복 Select, 아래 주석). */
    disabled?: boolean;
    conflictRule?: (cause: unknown) => RuleValue | undefined;
    oncomparison?: (open: boolean) => void;
  }

  const {
    idPrefix,
    rule,
    canEdit,
    zoneNote,
    onsave,
    untilError,
    draftName,
    onunsaved,
    ondraftkept,
    onsaving,
    disabled = false,
    conflictRule,
    oncomparison,
  }: Props = $props();

  const FREQ_LABEL: Record<Frequency | 'none', string> = {
    get none() {
      return m.events_freq_none();
    },
    get daily() {
      return m.events_rule_daily();
    },
    get weekly() {
      return m.events_rule_weekly();
    },
    get monthly() {
      return m.events_rule_monthly();
    },
    get yearly() {
      return m.events_rule_yearly();
    },
  };

  function initial(value: RecurrenceRule | null = rule): {
    freq: Frequency | 'none';
    interval: string;
    weekdays: number[];
    end: 'never' | 'until' | 'count';
    until: string;
    count: string;
  } {
    return {
      freq: value?.freq ?? 'none',
      interval: String(value?.interval ?? 1),
      weekdays: value?.weekdays ?? [],
      end: value?.count != null ? 'count' : value?.until != null ? 'until' : 'never',
      until: value?.until ?? '',
      count: String(value?.count ?? 10),
    };
  }

  const f = $state(initial());
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  let localUntilError = $state<string | undefined>(undefined);
  let conflicted = $state(false);
  let baseFields = $state(initial());
  let baseRule = $state.raw(untrack(() => rule));
  let comparison = $state<RuleComparison>();
  const fields = $derived(ruleConflictFields());
  const dirty = $derived(
    comparison !== undefined || JSON.stringify(f) !== JSON.stringify(baseFields),
  );
  function rawSnapshot(): RuleRawDraft {
    return $state.snapshot({ fields: f, baseFields, base: baseRule });
  }
  function restoreInput(value: RuleRawDraft): void {
    Object.assign(f, value.fields);
    baseFields = value.baseFields;
    baseRule = value.base;
    error = undefined;
    conflicted = false;
  }
  const draft = createFormDraft<RuleDraft>({
    name: () => draftName,
    snapshot: () => ({ ...rawSnapshot(), comparison }),
    parse: parseRuleDraft,
    restore: (value) => {
      restoreInput(value);
      comparison = value.comparison;
    },
    reset: () => {
      Object.assign(f, initial());
      baseFields = initial();
      baseRule = rule;
      error = undefined;
      busy = false;
      conflicted = false;
      comparison = undefined;
      localUntilError = undefined;
    },
    dirty: () => dirty,
    busy: () => busy,
  });
  export function discardDraft(): void {
    if (busy) return;
    const attempt = draft.capture();
    if (attempt !== undefined && draft.complete(attempt)) return;
    Object.assign(f, baseFields);
    draft.persist();
  }
  export function isSaving(): boolean {
    return busy;
  }
  $effect(() => {
    onunsaved?.(dirty);
    oncomparison?.(comparison?.open === true);
    ondraftkept?.(draft.kept);
    onsaving?.(busy);
  });
  onDestroy(() => {
    onunsaved?.(false);
    oncomparison?.(false);
    ondraftkept?.(true);
    onsaving?.(false);
  });

  /** 입력을 규칙으로. 틀린 값은 조용히 바꾸지 않고 오류 글로 돌려준다. */
  function build(): RecurrenceRule | null | string {
    if (f.freq === 'none') return null;
    const interval = Number(f.interval.trim());
    if (!Number.isInteger(interval) || interval < 1 || interval > 99)
      return m.events_interval_invalid();
    let count: number | null = null;
    if (f.end === 'count') {
      count = Number(f.count.trim());
      if (!Number.isInteger(count) || count < 1 || count > 1000) return m.events_count_invalid();
    }
    if (f.end === 'until' && f.until === '') return m.events_until_required();
    return {
      freq: f.freq,
      interval,
      weekdays: f.freq === 'weekly' ? [...f.weekdays].sort((a, b) => a - b) : [],
      until: f.end === 'until' ? f.until : null,
      count,
    };
  }

  async function save(event?: SubmitEvent, selected = false): Promise<void> {
    event?.preventDefault();
    if (busy || disabled || !canEdit) return;
    const attempt = draftName === undefined ? undefined : draft.capture();
    if (draftName !== undefined && attempt === undefined) return;
    error = undefined;
    localUntilError = undefined;
    const proposed = build();
    if (typeof proposed === 'string') {
      error = proposed;
      return;
    }
    busy = true;
    const raw = rawSnapshot();
    const savedFields = { ...f, weekdays: [...f.weekdays] };
    try {
      if (!(selected && comparison !== undefined && sameRule(proposed, comparison.server.rule)))
        await onsave(proposed, baseRule);
      if (attempt !== undefined && !draft.complete(attempt)) return;
      Object.assign(f, savedFields);
      baseFields = savedFields;
      baseRule = proposed;
      draft.persist();
      conflicted = false;
      comparison = undefined;
      notifyDone(m.common_saved());
    } catch (e: unknown) {
      if (attempt !== undefined && !draft.isOwner(attempt)) return;
      const current = conflictRule?.(e);
      if (current !== undefined) {
        const old = comparison;
        comparison = {
          base: { rule: baseRule },
          mine: { rule: proposed },
          server: current,
          mineDraft: raw,
          original: old?.original ?? raw,
          choices: initialFieldChoices(
            fieldDifferences(fields, { rule: baseRule }, { rule: proposed }, current),
          ),
          open: true,
          repeated: old !== undefined,
        };
        return;
      }
      // #76: 반복 끝이 첫 회차보다 앞서면 반복 끝 칸 옆에 구체 이유를 보인다.
      if (untilBeforeStart(e)) {
        localUntilError = m.events_until_before_start();
        error = comparison?.open === true ? localUntilError : undefined;
        return;
      }
      conflicted = e instanceof ApiError && e.code === 'rule_conflict';
      if (conflicted) baseRule = rule;
      // 그사이 다른 사람이 규칙을 바꿨다(서버 409). 입력한 칸은 그대로 두고, 요약(서버 규칙)과 함께 알린다.
      error =
        e instanceof ApiError && e.code === 'rule_conflict'
          ? m.events_rule_changed_meanwhile()
          : errorText(e, m.events_recurrence_save_failed());
    } finally {
      if (attempt === undefined || draft.lockedBy(attempt)) busy = false;
    }
  }
  export function cancelComparison(original = false): void {
    if (comparison?.open !== true || busy) return;
    restoreInput($state.snapshot(original ? comparison.original : comparison.mineDraft));
    comparison.open = false;
  }
  async function applyChoices(saveNow: boolean): Promise<void> {
    const ask = comparison;
    if (ask?.open !== true || busy || disabled || !canEdit) return;
    const candidate = selectedFields(
      fields,
      $state.snapshot(ask.mine),
      $state.snapshot(ask.server),
      ask.choices,
    );
    Object.assign(f, initial(candidate.rule));
    baseFields = initial(candidate.rule);
    baseRule = $state.snapshot(ask.server.rule);
    if (saveNow) await save(undefined, true);
    else ask.open = false;
  }
</script>

<section class="flex flex-col gap-2" aria-labelledby={`${idPrefix}-heading`}>
  <!-- 저장하는 동안 입력과 되살리기를 잠근다(D-97). 그사이 바뀐 입력은 성공한 저장을 버리게 하고, 기준 규칙을 옮기지
       않은 다음 저장이 자기 저장과 충돌한다. 일정 대화상자는 바깥에서도 잠그지만 업무 상세는 이 잠금만 쓴다. -->
  {#each draft.offers as offer (offer.key)}
    <fieldset class="contents" disabled={busy}>
      <FormDraftOffer
        onrestore={() => {
          draft.restore(offer);
        }}
        ondiscard={() => {
          draft.discard(offer);
        }}
      />
    </fieldset>
  {/each}
  <h3 id={`${idPrefix}-heading`} class="text-sm font-semibold">{m.events_recurrence()}</h3>
  <p class="text-sm text-muted-foreground">
    {rule === null ? m.events_no_recurrence() : describeRule(rule)} ({zoneNote})
  </p>
  {#if comparison?.open}
    <FieldConflict
      {fields}
      base={comparison.base}
      mine={comparison.mine}
      server={comparison.server}
      bind:choices={comparison.choices}
      busy={busy || disabled}
      blocked={!canEdit}
      repeated={comparison.repeated}
      onsave={() => void applyChoices(true)}
      onedit={() => void applyChoices(false)}
      oncancel={() => {
        cancelComparison();
      }}
      onoriginal={JSON.stringify(comparison.original) !== JSON.stringify(comparison.mineDraft)
        ? () => {
            cancelComparison(true);
          }
        : undefined}
    />
  {/if}
  {#if (canEdit || comparison !== undefined) && (draftName === undefined || draft.active)}
    <fieldset
      class="min-w-0"
      disabled={busy || disabled || !canEdit}
      inert={comparison?.open === true}
    >
      <form class="flex flex-wrap items-end gap-3" onsubmit={(e) => void save(e)}>
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium" id={`${idPrefix}-freq-label`}
            >{m.events_frequency()}</span
          >
          <!-- bits-ui Select는 조상 fieldset의 disabled를 보지 않고 열리므로 이 편집기의 저장(busy)과 부모의 저장(disabled)으로 따로 잠근다. -->
          <Select.Root
            type="single"
            disabled={busy || disabled}
            value={f.freq}
            onValueChange={(v: string) => {
              const found = FREQUENCIES.find((x) => x === v);
              f.freq = found ?? 'none';
            }}
          >
            <Select.Trigger aria-labelledby={`${idPrefix}-freq-label`}
              >{FREQ_LABEL[f.freq]}</Select.Trigger
            >
            <Select.Content>
              <Select.Item value="none" label={FREQ_LABEL.none}>{FREQ_LABEL.none}</Select.Item>
              {#each FREQUENCIES as fr (fr)}
                <Select.Item value={fr} label={FREQ_LABEL[fr]}>{FREQ_LABEL[fr]}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
        {#if f.freq !== 'none'}
          <div class="flex flex-col gap-1">
            <Label for={`${idPrefix}-interval`}>{m.events_interval()}</Label>
            <Input
              id={`${idPrefix}-interval`}
              class="w-20"
              inputmode="numeric"
              bind:value={f.interval}
            />
          </div>
          {#if f.freq === 'weekly'}
            <fieldset class="flex flex-col gap-1">
              <legend class="text-sm font-medium">{m.events_weekdays_legend()}</legend>
              <div class="flex flex-wrap gap-2">
                {#each ISO_WEEKDAYS as day (day)}
                  <span class="flex items-center gap-1">
                    <Checkbox
                      id={`${idPrefix}-wd-${String(day)}`}
                      checked={f.weekdays.includes(day)}
                      onCheckedChange={(v) => {
                        f.weekdays =
                          v === true
                            ? [...new Set([...f.weekdays, day])]
                            : f.weekdays.filter((d) => d !== day);
                      }}
                    />
                    <Label for={`${idPrefix}-wd-${String(day)}`}>{weekdayLabel(day)}</Label>
                  </span>
                {/each}
              </div>
            </fieldset>
          {/if}
          <fieldset class="flex flex-col gap-1">
            <legend class="text-sm font-medium">{m.events_end()}</legend>
            <div class="flex flex-wrap items-center gap-3 text-sm">
              <label class="flex items-center gap-1"
                ><input type="radio" bind:group={f.end} value="never" />
                {m.events_end_never()}</label
              >
              <label class="flex items-center gap-1"
                ><input type="radio" bind:group={f.end} value="until" />
                {m.events_end_until()}</label
              >
              {#if f.end === 'until'}
                <Input
                  class="w-40"
                  type="date"
                  bind:value={f.until}
                  aria-label={m.events_end_until_aria()}
                  aria-invalid={untilError !== undefined || localUntilError !== undefined}
                  aria-describedby={untilError !== undefined || localUntilError !== undefined
                    ? `${idPrefix}-until-error`
                    : undefined}
                  required
                />
                {#if untilError !== undefined || localUntilError !== undefined}
                  <p id={`${idPrefix}-until-error`} role="alert" class="text-sm text-destructive">
                    {untilError ?? localUntilError}
                  </p>
                {/if}
              {/if}
              <label class="flex items-center gap-1"
                ><input type="radio" bind:group={f.end} value="count" />
                {m.events_end_count()}</label
              >
              {#if f.end === 'count'}
                <Input
                  class="w-24"
                  inputmode="numeric"
                  bind:value={f.count}
                  aria-label={m.events_end_count_aria()}
                />
              {/if}
            </div>
          </fieldset>
        {/if}
        <Button
          type="submit"
          size="sm"
          disabled={busy}
          aria-label={conflicted ? m.tasks_rule_save_mine() : undefined}
          >{conflicted ? m.tasks_save_mine() : m.events_recurrence_save()}</Button
        >
      </form>
    </fieldset>
    {#if error !== undefined}
      <p role="alert" class="text-sm text-destructive">{error}</p>
    {/if}
  {/if}
</section>
