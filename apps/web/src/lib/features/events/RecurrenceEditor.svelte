<script lang="ts">
  // 반복 규칙 입력(업무·일정 공통, D-31). 단순한 규칙만: 매일·매주(요일)·매달·매년, 간격, 끝(없음·날짜·횟수).
  // 없는 날(31일·2월 29일)은 건너뛴다. 저장하면 이미 만든 회차는 바뀌지 않는다.
  import type { Frequency, RecurrenceRule } from '$lib/api/client';
  import { FREQUENCIES } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import { describeRule, ISO_WEEKDAYS, weekdayLabel } from './labels';

  interface Props {
    idPrefix: string;
    rule: RecurrenceRule | null;
    canEdit: boolean;
    /** 이 반복의 시간대(업무는 "오늘"의 기준, 일정은 일정 시간대). */
    zoneNote: string;
    onsave: (rule: RecurrenceRule | null) => Promise<void>;
  }

  const { idPrefix, rule, canEdit, zoneNote, onsave }: Props = $props();

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

  function initial(): {
    freq: Frequency | 'none';
    interval: string;
    weekdays: number[];
    end: 'never' | 'until' | 'count';
    until: string;
    count: string;
  } {
    return {
      freq: rule?.freq ?? 'none',
      interval: String(rule?.interval ?? 1),
      weekdays: rule?.weekdays ?? [],
      end: rule?.count != null ? 'count' : rule?.until != null ? 'until' : 'never',
      until: rule?.until ?? '',
      count: String(rule?.count ?? 10),
    };
  }

  const f = $state(initial());
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  let saved = $state(false);

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

  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (busy) return;
    error = undefined;
    saved = false;
    const rule = build();
    if (typeof rule === 'string') {
      error = rule;
      return;
    }
    busy = true;
    try {
      await onsave(rule);
      saved = true;
    } catch (e: unknown) {
      error = errorText(e, m.events_recurrence_save_failed());
    } finally {
      busy = false;
    }
  }
</script>

<section class="flex flex-col gap-2" aria-labelledby={`${idPrefix}-heading`}>
  <h3 id={`${idPrefix}-heading`} class="text-sm font-semibold">{m.events_recurrence()}</h3>
  <p class="text-sm text-muted-foreground">
    {rule === null ? m.events_no_recurrence() : describeRule(rule)} ({zoneNote})
  </p>
  {#if canEdit}
    <form class="flex flex-wrap items-end gap-3" onsubmit={(e) => void save(e)}>
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium" id={`${idPrefix}-freq-label`}>{m.events_frequency()}</span
        >
        <Select.Root
          type="single"
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
                required
              />
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
      <Button type="submit" size="sm" disabled={busy}>{m.events_recurrence_save()}</Button>
      {#if saved}<span class="text-sm" role="status">{m.common_saved()}</span>{/if}
    </form>
    {#if error !== undefined}
      <p role="alert" class="text-sm text-destructive">{error}</p>
    {/if}
  {/if}
</section>
