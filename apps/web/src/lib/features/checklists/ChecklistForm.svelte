<script lang="ts">
  // 에디터가 없는 화면용 일반 체크리스트 폼 (결정 D-30). 저장·권한 연결은 첫 사용자 흐름에서 붙인다.
  // 문서 편집기의 체크리스트와 같은 데이터·권한 규칙을 따라야 하며, 여기서는 표시와 입력만 담당한다.
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  import type { ChecklistItem } from './checklist-item';

  interface Props {
    items: ChecklistItem[];
    onchange?: (items: ChecklistItem[]) => void;
  }

  let { items, onchange }: Props = $props();

  let draft = $state('');
  let nextId = $state(0);

  function emit(next: ChecklistItem[]): void {
    items = next;
    onchange?.(next);
  }

  function add(): void {
    const text = draft.trim();
    if (text === '') return;
    nextId += 1;
    emit([...items, { id: `local-${String(nextId)}`, text, done: false }]);
    draft = '';
  }

  function toggle(id: string, done: boolean): void {
    emit(items.map((item) => (item.id === id ? { ...item, done } : item)));
  }

  function remove(id: string): void {
    emit(items.filter((item) => item.id !== id));
  }
</script>

<section aria-labelledby="checklist-heading" class="flex flex-col gap-3">
  <h2 id="checklist-heading" class="text-base font-semibold">체크리스트</h2>

  <ul class="flex flex-col gap-2" data-testid="checklist-items">
    {#each items as item (item.id)}
      <li class="flex items-center gap-2">
        <Checkbox
          id={`check-${item.id}`}
          checked={item.done}
          onCheckedChange={(value) => {
            toggle(item.id, value === true);
          }}
          aria-label={item.text}
        />
        <Label
          for={`check-${item.id}`}
          class={item.done ? 'line-through text-muted-foreground' : ''}
        >
          {item.text}
        </Label>
        <Button
          variant="ghost"
          size="sm"
          class="ml-auto"
          aria-label={`${item.text} 삭제`}
          onclick={() => {
            remove(item.id);
          }}
        >
          삭제
        </Button>
      </li>
    {:else}
      <li class="text-sm text-muted-foreground">항목이 없습니다.</li>
    {/each}
  </ul>

  <form
    class="flex items-end gap-2"
    onsubmit={(event) => {
      event.preventDefault();
      add();
    }}
  >
    <div class="flex flex-1 flex-col gap-1">
      <Label for="checklist-new">새 항목</Label>
      <Input id="checklist-new" bind:value={draft} placeholder="할 일을 입력" autocomplete="off" />
    </div>
    <Button type="submit" disabled={draft.trim() === ''}>추가</Button>
  </form>
</section>
