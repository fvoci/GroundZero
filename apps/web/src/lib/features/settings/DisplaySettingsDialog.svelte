<script lang="ts">
  // 표시 설정(D-50 시간대, D-81 언어·주 시작·시각 표시). 칸마다 따로 고르고 계정에 저장해 기기 사이에 같다. 아직
  // 고르지 않은 칸은 브라우저 언어·시간대와 그 언어의 관례로 제안한다(저장하기 전까지는 제안만 쓴다). 표시만 바꾸며
  // 저장된 일정 시각·날짜는 그대로다.
  import { api, ApiError } from '$lib/api/client';
  import type { DisplayLocale, HourCycle, WeekStart } from '$lib/display';
  import {
    asHourCycle,
    asLocale,
    asWeekStart,
    conventionalHourCycle,
    conventionalWeekStart,
    formatClock,
    suggestedSettings,
  } from '$lib/display';
  import { me } from '$lib/me.svelte';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';

  let { open = $bindable(false) }: { open?: boolean } = $props();

  const suggested = suggestedSettings();
  const zones: string[] =
    typeof Intl.supportedValuesOf === 'function'
      ? Intl.supportedValuesOf('timeZone')
      : [suggested.timeZone];
  let locale = $state<DisplayLocale>('ko');
  let zone = $state('');
  let weekStart = $state<WeekStart>('sun');
  let hourCycle = $state<HourCycle>('h12');
  let error = $state<string | undefined>(undefined);
  const unsaved = $derived(me.locale === '' || me.weekStart === '' || me.hourCycle === '');

  $effect(() => {
    if (open) {
      locale = asLocale(me.locale) ?? suggested.locale;
      zone = me.timeZone;
      weekStart = asWeekStart(me.weekStart) ?? conventionalWeekStart(locale);
      hourCycle = asHourCycle(me.hourCycle) ?? conventionalHourCycle(locale);
      error = undefined;
    }
  });

  /** 언어를 바꾸면, 아직 저장한 적 없는 주 시작·시각 표시는 그 언어의 관례로 다시 제안한다. */
  function changeLocale(next: DisplayLocale): void {
    locale = next;
    if (me.weekStart === '') weekStart = conventionalWeekStart(next);
    if (me.hourCycle === '') hourCycle = conventionalHourCycle(next);
  }

  const sample = $derived(
    formatClock('2026-01-01T15:05:00Z', {
      locale,
      timeZone: 'UTC',
      weekStart,
      hourCycle,
    }),
  );

  async function save(): Promise<void> {
    try {
      await api.setDisplaySettings({
        time_zone: zone,
        locale,
        week_start: weekStart,
        hour_cycle: hourCycle,
      });
      me.timeZone = zone;
      me.locale = locale;
      me.weekStart = weekStart;
      me.hourCycle = hourCycle;
      open = false;
      // 날짜 계산·서식을 새 설정으로 다시 하도록 지금 화면을 새로 읽는다.
      window.location.reload();
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '저장하지 못했습니다.';
    }
  }

  const fieldClass = 'h-9 rounded-md border border-input bg-transparent px-3 text-base';
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-md">
    <Dialog.Header>
      <Dialog.Title>표시 설정</Dialog.Title>
      <Dialog.Description
        >언어·시간대·한 주 시작·시각 표시를 고릅니다. 표시만 바뀌고 저장된 일정의 시각과 날짜는
        그대로입니다. 다른 사람에게는 보이지 않습니다.</Dialog.Description
      >
    </Dialog.Header>
    {#if unsaved}
      <p class="text-sm text-muted-foreground" role="status">
        아직 저장하지 않은 칸은 이 브라우저의 언어·시간대로 제안한 값입니다.
      </p>
    {/if}
    <div class="flex flex-col gap-3 text-sm">
      <label class="flex flex-col gap-1">
        <span class="font-medium">언어</span>
        <select
          class={fieldClass}
          bind:value={
            () => locale,
            (value: string) => {
              const next = asLocale(value);
              if (next !== undefined) changeLocale(next);
            }
          }
        >
          <option value="ko">한국어</option>
          <option value="en">English</option>
        </select>
      </label>
      <label class="flex flex-col gap-1">
        <span class="font-medium">시간대</span>
        <select class={fieldClass} bind:value={zone}>
          <option value="">브라우저 시간대 사용({suggested.timeZone})</option>
          {#each zones as z (z)}<option value={z}>{z}</option>{/each}
        </select>
      </label>
      <label class="flex flex-col gap-1">
        <span class="font-medium">한 주 시작</span>
        <select class={fieldClass} bind:value={weekStart}>
          <option value="mon">월요일</option>
          <option value="sun">일요일</option>
        </select>
      </label>
      <label class="flex flex-col gap-1">
        <span class="font-medium">시각 표시</span>
        <select class={fieldClass} bind:value={hourCycle}>
          <option value="h12">12시간(오전·오후)</option>
          <option value="h23">24시간</option>
        </select>
        <span class="text-xs text-muted-foreground">예: {sample}</span>
      </label>
    </div>
    {#if error !== undefined}<p role="alert" class="text-sm text-destructive">{error}</p>{/if}
    <div class="flex justify-end gap-2">
      <Button variant="ghost" onclick={() => (open = false)}>취소</Button>
      <Button onclick={() => void save()}>저장</Button>
    </div>
  </Dialog.Content>
</Dialog.Root>
