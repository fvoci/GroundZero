<script lang="ts">
  // 반복 규칙 입력(업무·일정 공통, D-31). 단순한 규칙만: 매일·매주(요일)·매달·매년, 간격, 끝(없음·날짜·횟수).
  // 없는 날(31일·2월 29일)은 건너뛴다. 저장하면 이미 만든 회차는 바뀌지 않는다.
  import type { Frequency, RecurrenceRule } from '$lib/api/client';
  import { FREQUENCIES } from '$lib/api/client';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import { describeRule, WEEKDAY_LABEL } from './labels';

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
    none: '반복 안 함',
    daily: '매일',
    weekly: '매주',
    monthly: '매달',
    yearly: '매년',
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
      return '간격은 1~99 사이의 정수여야 합니다.';
    let count: number | null = null;
    if (f.end === 'count') {
      count = Number(f.count.trim());
      if (!Number.isInteger(count) || count < 1 || count > 1000)
        return '횟수는 1~1000 사이의 정수여야 합니다.';
    }
    if (f.end === 'until' && f.until === '') return '끝 날짜를 고르세요.';
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
      error = e instanceof Error ? e.message : '반복을 저장하지 못했습니다.';
    } finally {
      busy = false;
    }
  }
</script>

<section class="flex flex-col gap-2" aria-labelledby={`${idPrefix}-heading`}>
  <h3 id={`${idPrefix}-heading`} class="text-sm font-semibold">반복</h3>
  <p class="text-sm text-muted-foreground">
    {rule === null ? '반복하지 않습니다.' : describeRule(rule)} ({zoneNote})
  </p>
  {#if canEdit}
    <form class="flex flex-wrap items-end gap-3" onsubmit={(e) => void save(e)}>
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium" id={`${idPrefix}-freq-label`}>주기</span>
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
          <Label for={`${idPrefix}-interval`}>간격</Label>
          <Input
            id={`${idPrefix}-interval`}
            class="w-20"
            inputmode="numeric"
            bind:value={f.interval}
          />
        </div>
        {#if f.freq === 'weekly'}
          <fieldset class="flex flex-col gap-1">
            <legend class="text-sm font-medium">요일(비우면 기준일 요일)</legend>
            <div class="flex flex-wrap gap-2">
              {#each WEEKDAY_LABEL as label, i (label)}
                <span class="flex items-center gap-1">
                  <Checkbox
                    id={`${idPrefix}-wd-${String(i + 1)}`}
                    checked={f.weekdays.includes(i + 1)}
                    onCheckedChange={(v) => {
                      f.weekdays =
                        v === true
                          ? [...new Set([...f.weekdays, i + 1])]
                          : f.weekdays.filter((d) => d !== i + 1);
                    }}
                  />
                  <Label for={`${idPrefix}-wd-${String(i + 1)}`}>{label}</Label>
                </span>
              {/each}
            </div>
          </fieldset>
        {/if}
        <fieldset class="flex flex-col gap-1">
          <legend class="text-sm font-medium">끝</legend>
          <div class="flex flex-wrap items-center gap-3 text-sm">
            <label class="flex items-center gap-1"
              ><input type="radio" bind:group={f.end} value="never" /> 없음</label
            >
            <label class="flex items-center gap-1"
              ><input type="radio" bind:group={f.end} value="until" /> 날짜까지</label
            >
            {#if f.end === 'until'}
              <Input
                class="w-40"
                type="date"
                bind:value={f.until}
                aria-label="반복 끝 날짜"
                required
              />
            {/if}
            <label class="flex items-center gap-1"
              ><input type="radio" bind:group={f.end} value="count" /> 횟수</label
            >
            {#if f.end === 'count'}
              <Input
                class="w-24"
                inputmode="numeric"
                bind:value={f.count}
                aria-label="반복 횟수(첫 회차 포함)"
              />
            {/if}
          </div>
        </fieldset>
      {/if}
      <Button type="submit" size="sm" disabled={busy}>반복 저장</Button>
      {#if saved}<span class="text-sm" role="status">저장했습니다.</span>{/if}
    </form>
    {#if error !== undefined}
      <p role="alert" class="text-sm text-destructive">{error}</p>
    {/if}
  {/if}
</section>
