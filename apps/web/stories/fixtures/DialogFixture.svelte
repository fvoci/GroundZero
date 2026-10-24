<script lang="ts">
  // 공통 대화상자(src/lib/ui/dialog)를 제품 화면과 같은 방식으로 조립한 합성 예. 긴 대화상자는 제품처럼
  // `max-h-[90vh] overflow-y-auto`를 넘긴다(TaskBoard·EventDialog·TimeEntryDialog).
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  interface Props {
    title: string;
    description: string;
    /** 본문에 더할 긴 안내 문단 수(내부 스크롤 확인). */
    paragraphs?: number;
    contentClass?: string;
    onsave: (title: string) => void;
  }

  const { title, description, paragraphs = 0, contentClass, onsave }: Props = $props();

  let open = $state(false);
  let value = $state('');
</script>

<Dialog.Root bind:open>
  <Dialog.Trigger>
    {#snippet child({ props })}
      <Button variant="outline" {...props}>업무 편집</Button>
    {/snippet}
  </Dialog.Trigger>
  <Dialog.Content class={contentClass}>
    <Dialog.Header>
      <Dialog.Title>{title}</Dialog.Title>
      <Dialog.Description>{description}</Dialog.Description>
    </Dialog.Header>
    <form
      id="story-dialog-form"
      class="flex flex-col gap-2"
      onsubmit={(e: SubmitEvent) => {
        e.preventDefault();
        onsave(value);
        open = false;
      }}
    >
      <Label for="story-dialog-title">제목</Label>
      <Input id="story-dialog-title" bind:value />
      {#each { length: paragraphs }, i (i)}
        <p class="text-muted-foreground">
          {i + 1}. 공동 작업 공간의 업무 설명 예시 문단입니다. 긴 설명이 대화상자 안에서 줄바꿈되고,
          화면보다 길어지면 대화상자 안에서 스크롤되는지 확인합니다.
        </p>
      {/each}
    </form>
    <Dialog.Footer>
      <Button variant="outline" onclick={() => (open = false)}>취소</Button>
      <Button type="submit" form="story-dialog-form">저장</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
