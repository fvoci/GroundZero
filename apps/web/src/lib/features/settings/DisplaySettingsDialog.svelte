<script lang="ts">
  // 표시 설정(D-50 시간대, D-81 언어·주 시작·시각 표시). 칸마다 따로 고르고 계정에 저장해 기기 사이에 같다. 아직
  // 고르지 않은 칸은 브라우저 언어·시간대와 그 언어의 관례로 제안한다(저장하기 전까지는 제안만 쓴다). 표시만 바꾸며
  // 저장된 일정 시각·날짜는 그대로다.
  import { untrack } from 'svelte';

  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
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
  import { appliedDocFont, setDocFont } from '$lib/doc-font';
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
  // 저장한 시간대가 이 브라우저 목록에 없을 수 있다(엔진마다 다른 이름, 목록을 주지 않는 브라우저). 그래도 고른 값으로
  // 보이게 목록에 넣는다(없으면 칸이 비어 보인다).
  const zoneOptions = $derived(
    me.timeZone === '' || zones.includes(me.timeZone) ? zones : [...zones, me.timeZone].sort(),
  );
  let locale = $state<DisplayLocale>('ko');
  let zone = $state('');
  let weekStart = $state<WeekStart>('sun');
  let hourCycle = $state<HourCycle>('h12');
  // 문서 본문 서체(D-86)는 계정이 아니라 이 브라우저에만 둔다.
  let docFont = $state<DocFont>('sans');
  /** 대화상자를 열 때의 계정 칸(문서 본문 서체만 바꿨는지 가린다). */
  let opened = '';
  const accountFields = (): string => JSON.stringify([locale, zone, weekStart, hourCycle]);
  let error = $state<string | undefined>(undefined);
  const unsaved = $derived(me.locale === '' || me.weekStart === '' || me.hourCycle === '');

  $effect(() => {
    if (!open) return;
    // 열 때만 채운다. 채우며 읽는 칸(관례 제안·accountFields)과 me를 의존으로 잡으면, 고를 때마다 효과가 다시 돌아
    // 저장된 값으로 되돌린다.
    untrack(() => {
      locale = asLocale(me.locale) ?? suggested.locale;
      zone = me.timeZone;
      weekStart = asWeekStart(me.weekStart) ?? conventionalWeekStart(locale);
      hourCycle = asHourCycle(me.hourCycle) ?? conventionalHourCycle(locale);
      // 저장한 값이 아니라 지금 적용된 서체(저장소를 쓸 수 없으면 이 화면에만 적용돼 있다). 저장한 값을 보이면
      // 다른 칸을 바꿔 저장할 때 적용된 서체를 되돌린다.
      docFont = appliedDocFont();
      opened = accountFields();
      error = undefined;
    });
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
    // 문서 본문 서체는 이 브라우저에만 둔다: 계정 저장과 상관없이 바로 적용한다.
    const fontChanged = docFont !== appliedDocFont();
    setDocFont(docFont);
    if (fontChanged && accountFields() === opened) {
      // 서체만 바꿨으면 계정 설정을 쓰지 않고 새로 읽지도 않는다.
      open = false;
      return;
    }
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
      error = errorText(e, m.settings_save_failed());
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
          {#each zoneOptions as z (z)}<option value={z}>{z}</option>{/each}
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
