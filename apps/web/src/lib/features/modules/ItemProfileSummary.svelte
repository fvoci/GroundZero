<script lang="ts">
  // 항목 정보의 읽기 보기(D-33·D-34·D-36·D-38). 정보 패널의 평소 보기와 판 충돌 알림의 서버 최신본(D-104)이 함께 쓴다.
  import type { ModuleKind } from '@fvoci/api-client';

  import type { ItemProfileView } from '$lib/api/client';
  import { m } from '$lib/paraglide/messages.js';
  import { Badge } from '$lib/ui/badge';

  import { detailFields, itemStatusLabel, peopleLabels, periodText } from './profile';

  interface Props {
    profile: ItemProfileView;
    kind: ModuleKind;
  }

  const { profile: p, kind }: Props = $props();
  const fields = $derived(detailFields(kind));
  const labels = $derived(peopleLabels(kind));
</script>

<dl class="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 text-sm">
  <dt class="text-muted-foreground">{m.modules_status()}</dt>
  <dd><Badge variant="outline">{itemStatusLabel(p.status)}</Badge></dd>
  {#if p.starts_on !== null || p.ends_on !== null}
    <dt class="text-muted-foreground">{m.modules_period()}</dt>
    <dd>{periodText(p.starts_on, p.ends_on)}</dd>
  {/if}
  <dt class="text-muted-foreground">{labels.leader}</dt>
  <dd>{p.leader?.display_name ?? m.modules_none()}</dd>
  {#if p.goal !== ''}
    <dt class="text-muted-foreground">{m.modules_goal()}</dt>
    <dd class="whitespace-pre-wrap">{p.goal}</dd>
  {/if}
  {#if p.scope !== ''}
    <dt class="text-muted-foreground">{m.modules_scope()}</dt>
    <dd class="whitespace-pre-wrap">{p.scope}</dd>
  {/if}
  {#if (p.details.keywords ?? []).length > 0}
    <dt class="text-muted-foreground">{m.modules_keywords()}</dt>
    <dd>{(p.details.keywords ?? []).join(', ')}</dd>
  {/if}
  {#each fields as f (f.key)}
    {#if p.details[f.key] !== undefined}
      <dt class="text-muted-foreground">{f.label}</dt>
      <dd class="whitespace-pre-wrap">{p.details[f.key]}</dd>
    {/if}
  {/each}
</dl>
