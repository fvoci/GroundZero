<script lang="ts">
  // 공통 대화상자(src/lib/ui/dialog)를 제품 화면과 같은 방식으로 조립한 합성 예.
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  interface Props {
    title: string;
    description: string;
    onsave: (title: string) => void;
  }

  const { title, description, onsave }: Props = $props();

  let open = $state(false);
  let value = $state('');
</script>

<Dialog.Root bind:open>
  <Dialog.Trigger>
    {#snippet child({ props })}
      <Button variant="outline" {...props}>업무 편집</Button>
    {/snippet}
  </Dialog.Trigger>
  <Dialog.Content>
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
    </form>
    <Dialog.Footer>
      <Button variant="outline" onclick={() => (open = false)}>취소</Button>
      <Button type="submit" form="story-dialog-form">저장</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
