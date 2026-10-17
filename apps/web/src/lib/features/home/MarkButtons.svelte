<script lang="ts">
  // 홈에 고정·즐겨찾기(D-19). 본인만 보는 표시이며 볼 수 있는 대상에만 켤 수 있다.
  import type { MarkKind, MarkTarget } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import { Button } from '$lib/ui/button';

  interface Props {
    kind: MarkTarget;
    id: string;
  }

  const { kind, id }: Props = $props();

  let marks = $state<{ pin: boolean; favorite: boolean } | undefined>(undefined);
  let error = $state<string | undefined>(undefined);

  $effect(() => {
    const target = { kind, id };
    let cancelled = false;
    api
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
      marks = await api.setMark(kind, id, mark, !marks[mark]);
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '바꾸지 못했습니다.';
    }
  }
</script>

{#if marks !== undefined}
  <div class="flex items-center gap-1" role="group" aria-label="홈에 표시">
    <Button
      size="sm"
      variant={marks.pin ? 'secondary' : 'ghost'}
      aria-pressed={marks.pin}
      onclick={() => void toggle('pin')}>{marks.pin ? '고정됨' : '홈에 고정'}</Button
    >
    <Button
      size="sm"
      variant={marks.favorite ? 'secondary' : 'ghost'}
      aria-pressed={marks.favorite}
      onclick={() => void toggle('favorite')}>{marks.favorite ? '★ 즐겨찾기' : '☆ 즐겨찾기'}</Button
    >
    {#if error !== undefined}<span role="alert" class="text-sm text-destructive">{error}</span>{/if}
  </div>
{/if}
