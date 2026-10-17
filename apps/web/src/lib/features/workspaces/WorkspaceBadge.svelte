<script lang="ts">
  // 워크스페이스 이름 앞의 색 배지와 같은 이름일 때의 보조 정보 (D-17).
  import type { NamedWorkspace } from './badge';
  import { badgeColor, workspaceHint } from './badge';

  interface Props {
    workspace: NamedWorkspace;
    all: readonly NamedWorkspace[];
  }

  const { workspace, all }: Props = $props();
  const hint = $derived(workspaceHint(workspace, all));
</script>

<span class="inline-flex min-w-0 items-center gap-1.5">
  <span aria-hidden="true" class={`size-2 shrink-0 rounded-full ${badgeColor(workspace.id)}`}
  ></span>
  <span class="truncate">{workspace.name}</span>
  {#if hint !== null}
    <span
      class="shrink-0 font-mono text-xs text-muted-foreground"
      title="같은 이름의 워크스페이스를 구분하는 표시">{hint}</span
    >
  {/if}
</span>
