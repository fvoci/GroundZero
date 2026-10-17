<script lang="ts">
  // 감사 기록(D-57). 워크스페이스(관리자) 또는 내 개인 영역. 최신 먼저, 더 보기로 이어서.
  import type { Workspace } from '@fvoci/api-client';

  import type { AuditEntryView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import { localTimeZone, wallTime } from '$lib/features/events/time';
  import { Button } from '$lib/ui/button';

  import { AUDIT_ACTION, AUDIT_TARGET, auditSubject } from './labels';

  interface Props {
    /** 없으면 내 개인 영역 기록. */
    workspace: Workspace | undefined;
  }

  const { workspace }: Props = $props();

  const zone = localTimeZone();
  let entries = $state<AuditEntryView[]>([]);
  let error = $state<string | undefined>(undefined);
  let more = $state(false);
  let loading = $state(false);
  let seq = 0;

  async function load(before: string | undefined, mine: number): Promise<void> {
    loading = true;
    try {
      const page = await api.audit(workspace?.id ?? null, before);
      if (mine !== seq) return;
      entries = before === undefined ? page : [...entries, ...page];
      more = page.length === 100;
      error = undefined;
    } catch (e: unknown) {
      if (mine === seq) error = e instanceof ApiError ? e.message : '기록을 불러오지 못했습니다.';
    } finally {
      if (mine === seq) loading = false;
    }
  }

  $effect(() => {
    if (workspace?.id === '') return;
    seq += 1;
    void load(undefined, seq);
  });

  const when = (iso: string): string => wallTime(iso, zone).replace('T', ' ');
</script>

<section class="flex flex-col gap-3" aria-labelledby="audit-heading">
  <h1 id="audit-heading" class="text-xl font-semibold">
    {workspace === undefined ? '내 활동 기록' : '감사 기록'}
  </h1>
  <p class="text-sm text-muted-foreground">
    {workspace === undefined
      ? '개인 노트·개인 파일·반출처럼 내 개인 영역에서 있었던 일입니다. 다른 사람은 볼 수 없습니다.'
      : '이 워크스페이스에서 있었던 일입니다. 관리자만 봅니다. 멤버의 개인 노트는 들어가지 않습니다.'}
  </p>
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}
  <ol class="flex flex-col gap-1 text-sm" data-testid="audit-list">
    {#each entries as e (e.id)}
      <li class="flex flex-wrap items-baseline gap-2 border-b py-1">
        <span class="text-xs text-muted-foreground tabular-nums">{when(e.created_at)}</span>
        <span class="font-medium"
          >{e.actor_name ?? (e.actor === null ? '자동 작업' : '알 수 없음')}</span
        >
        <span
          >{AUDIT_TARGET[e.target_type] ?? e.target_type} {AUDIT_ACTION[e.action] ?? e.action}</span
        >
        {#if auditSubject(e.detail) !== ''}
          <span class="min-w-0 truncate text-muted-foreground">{auditSubject(e.detail)}</span>
        {/if}
      </li>
    {:else}
      {#if !loading}
        <li class="text-muted-foreground">기록이 없습니다.</li>
      {/if}
    {/each}
  </ol>
  {#if more}
    <div>
      <Button
        variant="outline"
        disabled={loading}
        onclick={() => {
          const last = entries.at(-1)?.id;
          seq += 1;
          void load(last, seq);
        }}>더 보기</Button
      >
    </div>
  {/if}
  <p aria-live="polite" class="sr-only">{loading ? '불러오는 중' : ''}</p>
</section>
