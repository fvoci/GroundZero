<script lang="ts">
  // 시간 기록 고치기(본인만): 시작·끝(보는 사람 시간대)·멈춘 시간·종류·메모·연결 대상, 수정 이력, 지우기.
  // 기준 버전으로 충돌을 막고, 충돌하면 서버 내용을 보여 준 뒤 다시 고치게 한다.
  import type { TimeEntryView, TimeKind, TimeTargetKind } from '$lib/api/client';
  import { api, ApiError, TIME_KINDS } from '$lib/api/client';
  import { instantOf, wallTime } from '$lib/features/events/time';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import { durationText, TIME_KIND_LABEL } from './format';
  import TargetPicker from './TargetPicker.svelte';

  interface Props {
    entry: TimeEntryView | undefined;
    zone: string;
    onclose: () => void;
    onchanged: () => void;
  }

  const { entry, zone, onclose, onchanged }: Props = $props();

  interface Form {
    start: string;
    end: string;
    pausedMinutes: string;
    kind: TimeKind;
    memo: string;
    target: { kind: TimeTargetKind; id: string; label: string } | null;
  }

  let form = $state<Form | undefined>(undefined);
  // 고치지 않은 칸은 원래 값(초 단위)을 그대로 보낸다. 입력란은 분 단위라 다시 계산하면 길이가 바뀐다.
  let initial = { start: '', end: '', pausedMinutes: '' };
  let base = $state<TimeEntryView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  let busy = $state(false);
  let confirmDelete = $state(false);
  let history = $state<
    {
      started_at: string;
      ended_at: string | null;
      kind: TimeKind;
      memo: string;
      changed_at: string;
    }[]
  >([]);

  function fill(e: TimeEntryView): void {
    base = e;
    form = {
      start: wallTime(e.started_at, zone),
      end: e.ended_at === null ? '' : wallTime(e.ended_at, zone),
      pausedMinutes: String(Math.round(e.paused_seconds / 60)),
      kind: e.kind,
      memo: e.memo,
      target:
        e.target === null
          ? null
          : { kind: e.target.kind, id: e.target.id, label: e.target.label ?? '볼 수 없는 대상' },
    };
    initial = { start: form.start, end: form.end, pausedMinutes: form.pausedMinutes };
  }

  $effect(() => {
    const e = entry;
    error = undefined;
    confirmDelete = false;
    history = [];
    if (e === undefined) return;
    fill(e);
    void api
      .timeEntryRevisions(e.id)
      .then((h) => {
        history = h;
      })
      .catch(() => {
        history = [];
      });
  });

  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (base === undefined || form === undefined || busy) return;
    const paused = Number(form.pausedMinutes.trim() === '' ? '0' : form.pausedMinutes);
    if (!Number.isInteger(paused) || paused < 0) {
      error = '멈춘 시간은 0 이상의 분이어야 합니다.';
      return;
    }
    busy = true;
    error = undefined;
    try {
      const result = await api.updateTimeEntry(base.id, base.version, {
        started_at: form.start === initial.start ? base.started_at : instantOf(form.start, zone),
        ended_at:
          form.end === ''
            ? null
            : form.end === initial.end && base.ended_at !== null
              ? base.ended_at
              : instantOf(form.end, zone),
        // 진행 중인 타이머와 고치지 않은 멈춘 시간은 원래 초 단위 값 그대로.
        paused_seconds:
          base.ended_at === null || form.pausedMinutes === initial.pausedMinutes
            ? base.paused_seconds
            : paused * 60,
        kind: form.kind,
        memo: form.memo,
        target: form.target === null ? null : { kind: form.target.kind, id: form.target.id },
      });
      if (result.kind === 'conflict') {
        fill(result.current);
        history = await api.timeEntryRevisions(result.current.id).catch(() => history);
        error = '그사이 이 기록이 바뀌었습니다. 지금 내용을 불러왔으니 다시 고쳐 주세요.';
        return;
      }
      onchanged();
      onclose();
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '저장하지 못했습니다.';
    } finally {
      busy = false;
    }
  }

  async function remove(): Promise<void> {
    if (base === undefined) return;
    try {
      await api.deleteTimeEntry(base.id);
      onchanged();
      onclose();
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '지우지 못했습니다.';
    }
  }
</script>

<Dialog.Root
  open={entry !== undefined}
  onOpenChange={(o: boolean) => {
    if (!o) onclose();
  }}
>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-xl">
    <Dialog.Header>
      <Dialog.Title>시간 기록 고치기</Dialog.Title>
      <Dialog.Description>
        {#if base !== undefined}{durationText(base.seconds)} · 시각은 {zone} 기준{/if}
      </Dialog.Description>
    </Dialog.Header>
    {#if form !== undefined && base !== undefined}
      {@const f = form}
      {@const running = base.ended_at === null}
      <form class="grid gap-3 sm:grid-cols-2" onsubmit={(e) => void save(e)}>
        <div class="flex flex-col gap-1">
          <Label for="time-edit-start">시작</Label>
          <Input id="time-edit-start" type="datetime-local" bind:value={f.start} required />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="time-edit-end">끝</Label>
          <Input
            id="time-edit-end"
            type="datetime-local"
            bind:value={f.end}
            disabled={running}
            required={!running}
          />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="time-edit-paused">멈춘 시간(분)</Label>
          <Input
            id="time-edit-paused"
            inputmode="numeric"
            bind:value={f.pausedMinutes}
            disabled={running}
          />
        </div>
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium" id="time-edit-kind-label">종류</span>
          <Select.Root
            type="single"
            value={f.kind}
            onValueChange={(v: string) => {
              const k = TIME_KINDS.find((x) => x === v);
              if (k !== undefined) f.kind = k;
            }}
          >
            <Select.Trigger aria-labelledby="time-edit-kind-label"
              >{TIME_KIND_LABEL[f.kind]}</Select.Trigger
            >
            <Select.Content>
              {#each TIME_KINDS as k (k)}
                <Select.Item value={k} label={TIME_KIND_LABEL[k]}>{TIME_KIND_LABEL[k]}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
        <div class="flex flex-col gap-1 sm:col-span-2">
          <Label for="time-edit-memo">메모</Label>
          <Input id="time-edit-memo" bind:value={f.memo} maxlength={2000} />
        </div>
        <div class="flex flex-col gap-2 sm:col-span-2">
          <span class="text-sm font-medium">연결 대상</span>
          {#if f.target !== null}
            <div class="flex items-center gap-2 text-sm">
              <span>{f.target.label}</span>
              <Button size="sm" variant="ghost" onclick={() => (f.target = null)}>연결 해제</Button>
            </div>
          {:else}
            <span class="text-sm text-muted-foreground">없음</span>
          {/if}
          <TargetPicker
            idPrefix="time-edit"
            onpick={(t: { kind: TimeTargetKind; id: string; label: string }) => {
              f.target = t;
            }}
          />
        </div>
        {#if error !== undefined}
          <p role="alert" class="text-sm text-destructive sm:col-span-2">{error}</p>
        {/if}
        <div class="flex flex-wrap items-center gap-2 sm:col-span-2">
          <Button type="submit" disabled={busy}>저장</Button>
          {#if confirmDelete}
            <span class="text-sm" role="status">이 기록을 지울까요?</span>
            <Button size="sm" variant="destructive" onclick={() => void remove()}>지우기</Button>
            <Button size="sm" variant="ghost" onclick={() => (confirmDelete = false)}>취소</Button>
          {:else}
            <Button size="sm" variant="outline" onclick={() => (confirmDelete = true)}
              >기록 지우기</Button
            >
          {/if}
        </div>
      </form>
      {#if history.length > 0}
        <section class="flex flex-col gap-1 border-t pt-2" aria-labelledby="time-history-heading">
          <h3 id="time-history-heading" class="text-sm font-semibold">수정 이력(고치기 전 값)</h3>
          <ul class="flex flex-col gap-1 text-sm text-muted-foreground">
            {#each history as h (h.changed_at)}
              <li>
                {wallTime(h.changed_at, zone).replace('T', ' ')}에 고침: {wallTime(
                  h.started_at,
                  zone,
                ).replace('T', ' ')} ~ {h.ended_at === null
                  ? '진행 중'
                  : wallTime(h.ended_at, zone).slice(11)} · {TIME_KIND_LABEL[h.kind]}{h.memo === ''
                  ? ''
                  : ` · ${h.memo}`}
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {/if}
  </Dialog.Content>
</Dialog.Root>
