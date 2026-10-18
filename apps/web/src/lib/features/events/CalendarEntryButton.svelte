<script lang="ts">
  // 달력 칸의 업무·일정·날짜 단추. 화면 낭독기에는 항목 이름과 완료 여부까지 읽힌다.
  import type { CalendarEntry } from './calendar';

  interface Props {
    entry: CalendarEntry;
    entryKey: string;
    itemTitle: string;
    onopen: () => void;
  }

  const { entry, entryKey, itemTitle, onopen }: Props = $props();
  const done = $derived(
    (entry.kind === 'task' && entry.task.status === 'done') ||
      (entry.kind === 'date' && entry.date.done),
  );
</script>

<button
  type="button"
  data-calendar-entry={entryKey}
  class={[
    'w-full truncate rounded px-1 text-left text-xs hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none',
    entry.kind === 'event' && 'bg-primary/10',
    entry.kind === 'date' && 'border-l-2 border-primary/60',
    entry.kind === 'projected' && 'border border-dashed text-muted-foreground italic',
    done && 'text-muted-foreground line-through',
  ]}
  title={`${entry.label} · ${itemTitle}`}
  aria-label={`${entry.label}, ${itemTitle}${done ? ', 완료' : ''}`}
  onclick={onopen}>{entry.label}</button
>
