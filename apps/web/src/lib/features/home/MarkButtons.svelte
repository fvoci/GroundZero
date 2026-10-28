<script lang="ts">
  import { activityApi, type MarkKind } from '$lib/features/activity/api';
  // 홈에 고정·즐겨찾기(D-19). 본인만 보는 표시이며 볼 수 있는 대상에만 켤 수 있다.
  import type { MarkTarget } from '$lib/features/activity/api';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import * as DropdownMenu from '$lib/ui/dropdown-menu';

  interface Props {
    kind: MarkTarget;
    id: string;
    /** `menu`: 문서 화면의 "더 보기" 메뉴 안에 체크 항목으로 그린다. 메뉴를 열 때마다 지금 표시를 다시 읽는다. */
    layout?: 'buttons' | 'menu';
  }

  const { kind, id, layout = 'buttons' }: Props = $props();

  let marks = $state<{ pin: boolean; favorite: boolean } | undefined>(undefined);
  let error = $state<string | undefined>(undefined);

  $effect(() => {
    const target = { kind, id };
    let cancelled = false;
    activityApi
      .mark(target.kind, target.id)
      .then((s) => {
        if (!cancelled) marks = s;
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  });

  async function toggle(mark: MarkKind): Promise<void> {
    if (marks === undefined) return;
    error = undefined;
    try {
      marks = await activityApi.setMark(kind, id, mark, !marks[mark]);
    } catch (e: unknown) {
      error = errorText(e, m.home_mark_failed());
    }
  }
</script>

{#if layout === 'menu'}
  <!-- 체크는 서버가 돌려준 표시를 따른다(바꾸지 못하면 그대로). 바꿔도 메뉴는 닫히지 않는다. -->
  <DropdownMenu.Group aria-label={m.home_marks_group()}>
    <DropdownMenu.CheckboxItem
      disabled={marks === undefined}
      bind:checked={() => marks?.pin ?? false, () => void toggle('pin')}
      >{m.home_pin()}</DropdownMenu.CheckboxItem
    >
    <DropdownMenu.CheckboxItem
      disabled={marks === undefined}
      bind:checked={() => marks?.favorite ?? false, () => void toggle('favorite')}
      >{m.home_favorite()}</DropdownMenu.CheckboxItem
    >
    {#if error !== undefined}<p role="alert" class="px-1.5 py-1 text-sm text-destructive">
        {error}
      </p>{/if}
  </DropdownMenu.Group>
{:else if marks !== undefined}
  <div
    class="flex min-w-0 flex-wrap items-center gap-1"
    role="group"
    aria-label={m.home_marks_group()}
  >
    <Button
      size="sm"
      variant={marks.pin ? 'secondary' : 'ghost'}
      aria-pressed={marks.pin}
      onclick={() => void toggle('pin')}>{m.home_pin()}</Button
    >
    <Button
      size="sm"
      variant={marks.favorite ? 'secondary' : 'ghost'}
      aria-pressed={marks.favorite}
      onclick={() => void toggle('favorite')}
      ><span aria-hidden="true">{marks.favorite ? '★' : '☆'}</span> {m.home_favorite()}</Button
    >
    {#if error !== undefined}<span role="alert" class="text-sm text-destructive">{error}</span>{/if}
  </div>
{/if}
