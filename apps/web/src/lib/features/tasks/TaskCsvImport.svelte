<script lang="ts">
  // 업무 CSV 일괄 등록(D-39). 한 줄씩 기존 업무 만들기·고치기 API로 넣는다(같은 검증과 권한). 줄마다 요청 id를
  // 기억해 두어 중간에 실패해도 다시 누르면 이미 만든 업무를 또 만들지 않는다.
  import type { AssigneeView } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import { Button } from '$lib/ui/button';
  import { Label } from '$lib/ui/label';

  import { parseTasksCsv } from './csv';

  interface Props {
    itemId: string;
    onimported: () => Promise<void>;
  }

  const { itemId, onimported }: Props = $props();

  let text = $state('');
  let assignable = $state<AssigneeView[]>([]);
  let status = $state('');
  let error = $state<string | undefined>(undefined);
  let busy = $state(false);
  // 같은 글자로 다시 가져오면 같은 요청 id를 쓴다(줄 순서 기준).
  let requestIds: string[] = [];
  let forText = '';

  $effect(() => {
    const id = itemId;
    api
      .assignable(id)
      .then((a) => (assignable = a))
      .catch(() => (assignable = []));
  });

  const parsed = $derived(text.trim() === '' ? undefined : parseTasksCsv(text, assignable));

  async function importRows(): Promise<void> {
    const result = parsed;
    if (result === undefined || result.errors.length > 0 || busy) return;
    if (forText !== text) {
      requestIds = result.rows.map(() => newRequestId());
      forText = text;
    }
    busy = true;
    error = undefined;
    let done = 0;
    try {
      for (const [index, row] of result.rows.entries()) {
        const created = await api.createTask(
          itemId,
          requestIds[index] ?? newRequestId(),
          row.title,
        );
        const updated = await api.updateTask(created.id, created.version, {
          title: row.title,
          description: row.description,
          status: row.status,
          priority: row.priority,
          start_date: row.start_date,
          due_date: row.due_date,
          estimate_minutes: row.estimate_minutes,
          tags: row.tags,
          assignee_ids: row.assignee_ids,
          milestone: false,
        });
        if (updated.kind !== 'updated')
          throw new Error(`${row.title}: 다른 변경과 겹쳤습니다. 다시 눌러 주세요.`);
        done += 1;
        status = `${String(done)} / ${String(result.rows.length)}개 넣는 중…`;
      }
      status = `업무 ${String(done)}개를 등록했습니다.`;
      text = '';
      forText = '';
    } catch (e: unknown) {
      error =
        (e instanceof ApiError || e instanceof Error ? e.message : '등록하지 못했습니다.') +
        ` (${String(done)}개 등록됨, 다시 누르면 나머지를 이어서 넣습니다)`;
    } finally {
      busy = false;
      await onimported();
    }
  }
</script>

<details class="text-sm">
  <summary class="cursor-pointer">CSV로 여러 업무 등록</summary>
  <div class="flex flex-col gap-2 pt-2">
    <Label for={`tasks-csv-${itemId}`}
      >한 줄에 한 업무: 제목, 설명, 상태(할 일·진행 중·완료), 우선순위(없음·낮음·보통·높음·긴급),
      시작일, 기한(YYYY-MM-DD), 예상(분), 담당자(이름, ;로 여럿), 태그(;로 여럿)</Label
    >
    <textarea
      id={`tasks-csv-${itemId}`}
      class="min-h-32 rounded-md border bg-background p-2 font-mono text-sm"
      bind:value={text}></textarea>
    <p aria-live="polite">
      {#if error !== undefined}
        <span class="text-destructive">{error}</span>
      {:else if parsed !== undefined}
        {parsed.rows.length}개 업무를 읽었습니다.
        {#if parsed.errors.length > 0}
          <span class="text-destructive">
            고칠 줄: {parsed.errors.map((er) => `${String(er.line)}번째(${er.reason})`).join(', ')}
          </span>
        {/if}
      {:else}
        {status}
      {/if}
    </p>
    <div>
      <Button
        variant="outline"
        disabled={busy ||
          parsed === undefined ||
          parsed.rows.length === 0 ||
          parsed.errors.length > 0}
        onclick={() => void importRows()}>등록</Button
      >
    </div>
  </div>
</details>
