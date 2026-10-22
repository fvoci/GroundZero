<script lang="ts">
  // 고친 시각이 그 시간대에서 두 번 있을 때(가을 여름시간 끝, D-93) 어느 쪽인지 고르는 묶음. 시각 입력란 바로 아래에 두고
  // 입력란의 aria-describedby가 이 묶음의 안내(legend)와 오류를 가리킨다(id는 입력란 id에서 만든다). 고르지 않고 저장하면
  // 묶음 아래에 알린다(D-82). 미리 골라 두지 않는다.
  import { m } from '$lib/paraglide/messages.js';

  import type { RepeatedOption } from './repeated';

  interface Props {
    /** 시각 입력란의 id. */
    id: string;
    /** 입력란 이름(시작·종료). */
    field: string;
    zone: string;
    options: RepeatedOption[];
    chosen: string | undefined;
    /** 고르지 않고 저장하려 했는지. */
    invalid: boolean;
    /** 고르지 않고 저장하려 한 횟수. 다시 막히면 알림을 새로 그려 화면 낭독기가 다시 읽게 한다. */
    attempt: number;
    onchoose: (instant: string) => void;
  }

  const { id, field, zone, options, chosen, invalid, attempt, onchoose }: Props = $props();
</script>

<fieldset
  class="flex flex-col gap-1 rounded-md border p-2 text-sm"
  aria-describedby={invalid ? `${id}-repeated-error` : undefined}
>
  <legend id={`${id}-repeated-legend`} class="px-1 text-xs"
    >{m.events_repeated_time_legend({ field, zone })}</legend
  >
  {#each options as o (o.instant)}
    <label class="flex items-center gap-2">
      <input
        type="radio"
        name={`${id}-repeated`}
        value={o.instant}
        checked={chosen === o.instant}
        onchange={() => {
          onchoose(o.instant);
        }}
      />
      {o.label}
    </label>
  {/each}
</fieldset>
{#if invalid}
  {#key attempt}
    <p id={`${id}-repeated-error`} class="text-xs text-destructive" role="alert">
      {m.events_repeated_time_required({ field })}
    </p>
  {/key}
{/if}
