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
  import type { DocFont } from '$lib/doc-font';
  import { setDocFont, storedDocFont } from '$lib/doc-font';
  import { me } from '$lib/me.svelte';
  import { m } from '$lib/paraglide/messages.js';
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
  // 문서 본문 서체(D-86)는 계정이 아니라 이 브라우저에만 둔다.
  let docFont = $state<DocFont>('sans');
  let error = $state<string | undefined>(undefined);
  const unsaved = $derived(me.locale === '' || me.weekStart === '' || me.hourCycle === '');

  $effect(() => {
    if (open) {
      locale = asLocale(me.locale) ?? suggested.locale;
      zone = me.timeZone;
      weekStart = asWeekStart(me.weekStart) ?? conventionalWeekStart(locale);
      hourCycle = asHourCycle(me.hourCycle) ?? conventionalHourCycle(locale);
      docFont = storedDocFont();
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
      setDocFont(docFont);
      open = false;
      // 날짜 계산·서식을 새 설정으로 다시 하도록 지금 화면을 새로 읽는다.
      window.location.reload();
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : m.settings_save_failed();
    }
  }

  const fieldClass = 'h-9 rounded-md border border-input bg-transparent px-3 text-base';
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-md">
    <Dialog.Header>
      <Dialog.Title>{m.settings_title()}</Dialog.Title>
      <Dialog.Description>{m.settings_description()}</Dialog.Description>
    </Dialog.Header>
    {#if unsaved}
      <p class="text-sm text-muted-foreground" role="status">{m.settings_unsaved_hint()}</p>
    {/if}
    <div class="flex flex-col gap-3 text-sm">
      <label class="flex flex-col gap-1">
        <span class="font-medium">{m.settings_language()}</span>
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
          <option value="ko">{m.settings_language_ko()}</option>
          <option value="en">{m.settings_language_en()}</option>
        </select>
      </label>
      <label class="flex flex-col gap-1">
        <span class="font-medium">{m.settings_time_zone()}</span>
        <select class={fieldClass} bind:value={zone}>
          <option value="">{m.settings_browser_zone({ zone: suggested.timeZone })}</option>
          {#each zones as z (z)}<option value={z}>{z}</option>{/each}
        </select>
      </label>
      <label class="flex flex-col gap-1">
        <span class="font-medium">{m.settings_week_start()}</span>
        <select class={fieldClass} bind:value={weekStart}>
          <option value="mon">{m.settings_monday()}</option>
          <option value="sun">{m.settings_sunday()}</option>
        </select>
      </label>
      <label class="flex flex-col gap-1">
        <span class="font-medium">{m.settings_clock()}</span>
        <select class={fieldClass} bind:value={hourCycle}>
          <option value="h12">{m.settings_clock_h12()}</option>
          <option value="h23">{m.settings_clock_h23()}</option>
        </select>
        <span class="text-xs text-muted-foreground">{m.settings_example({ sample })}</span>
      </label>
      <label class="flex flex-col gap-1">
        <span class="font-medium">{m.settings_doc_font()}</span>
        <select class={fieldClass} bind:value={docFont}>
          <option value="sans">{m.settings_doc_font_sans()}</option>
          <option value="serif">{m.settings_doc_font_serif()}</option>
        </select>
        <span class="text-xs text-muted-foreground">{m.settings_doc_font_hint()}</span>
      </label>
    </div>
    {#if error !== undefined}<p role="alert" class="text-sm text-destructive">{error}</p>{/if}
    <div class="flex justify-end gap-2">
      <Button variant="ghost" onclick={() => (open = false)}>{m.common_cancel()}</Button>
      <Button onclick={() => void save()}>{m.common_save()}</Button>
    </div>
  </Dialog.Content>
</Dialog.Root>
