<script lang="ts">
  // 달력 칸의 업무·일정·날짜 단추. 화면 낭독기에는 항목 이름과 완료 여부까지 읽힌다.
  import { m } from '$lib/paraglide/messages.js';

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
    // 좁은 화면의 날짜별 목록에서는 누르기 쉽게 높이를 1.5rem으로 둔다(달력 칸은 md 이상에서만 보인다).
    'w-full truncate rounded px-1 py-1 text-left text-xs hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none md:py-0',
    entry.kind === 'event' && 'bg-primary/10',
    entry.kind === 'date' && 'border-l-2 border-primary/60',
    entry.kind === 'projected' && 'border border-dashed text-muted-foreground italic',
    done && 'text-muted-foreground line-through',
  ]}
  title={`${entry.label} · ${itemTitle}`}
  aria-label={done
    ? m.events_entry_aria_done({ label: entry.label, item: itemTitle })
    : m.events_entry_aria({ label: entry.label, item: itemTitle })}
  onclick={onopen}>{entry.label}</button
>
