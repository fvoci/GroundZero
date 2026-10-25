<script lang="ts">
  // 모듈 종류 아이콘(D-121: 모듈 목록에 종류 아이콘). 이름은 함께 보이거나 title·접근성 이름으로 주므로 장식(aria-hidden)이다.
  import type { ModuleKind } from '@fvoci/api-client';
  import BookOpen from '@lucide/svelte/icons/book-open';
  import FlaskConical from '@lucide/svelte/icons/flask-conical';
  import FolderKanban from '@lucide/svelte/icons/folder-kanban';
  import GraduationCap from '@lucide/svelte/icons/graduation-cap';
  import Palette from '@lucide/svelte/icons/palette';

  interface Props {
    kind: ModuleKind;
    class?: string;
  }

  const { kind, class: className = '' }: Props = $props();
</script>

{#if kind === 'project'}
  <FolderKanban class={className} aria-hidden="true" />
{:else if kind === 'research'}
  <FlaskConical class={className} aria-hidden="true" />
{:else if kind === 'study'}
  <BookOpen class={className} aria-hidden="true" />
{:else if kind === 'course'}
  <GraduationCap class={className} aria-hidden="true" />
{:else}
  <Palette class={className} aria-hidden="true" />
{/if}
