<script lang="ts">
  // 감사 기록(D-57). 워크스페이스(관리자) 또는 내 개인 영역. 최신 먼저, 더 보기로 이어서.
  import type { Workspace } from '@fvoci/api-client';

  import type { AuditEntryView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { formatInstant } from '$lib/display';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import { auditAction, auditSubject, auditTarget } from './labels';

  interface Props {
    /** 없으면 내 개인 영역 기록. */
    workspace: Workspace | undefined;
  }

  const { workspace }: Props = $props();

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
      if (mine === seq) error = errorText(e, m.audit_load_failed());
    } finally {
      if (mine === seq) loading = false;
    }
  }

  $effect(() => {
    if (workspace?.id === '') return;
    seq += 1;
    void load(undefined, seq);
  });

  const when = (iso: string): string => formatInstant(iso);
</script>

<section class="flex flex-col gap-3" aria-labelledby="audit-heading">
  <h1 id="audit-heading" class="text-xl font-semibold">
    {workspace === undefined ? m.audit_my_heading() : m.audit_heading()}
  </h1>
  <p class="text-sm text-muted-foreground">
    {workspace === undefined ? m.audit_my_hint() : m.audit_hint()}
  </p>
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}
  <ol class="flex flex-col gap-1 text-sm" data-testid="audit-list">
    {#each entries as e (e.id)}
      {@const subject = auditSubject(e.detail, e.action)}
      <li class="flex flex-wrap items-baseline gap-2 border-b py-1">
        <span class="text-xs text-muted-foreground tabular-nums">{when(e.created_at)}</span>
        <span class="font-medium"
          >{e.actor_name ?? (e.actor === null ? m.audit_system() : m.audit_unknown())}</span
        >
        <span
          >{m.audit_entry({
            target: auditTarget(e.target_type),
            action: auditAction(e.action),
          })}</span
        >
        {#if subject !== ''}
          <span class="min-w-0 truncate text-muted-foreground" title={subject}>{subject}</span>
        {/if}
      </li>
    {:else}
      {#if !loading}
        <li class="text-muted-foreground">{m.audit_empty()}</li>
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
        }}>{m.audit_more()}</Button
      >
    </div>
  {/if}
  <p aria-live="polite" class="sr-only">{loading ? m.app_loading() : ''}</p>
</section>
