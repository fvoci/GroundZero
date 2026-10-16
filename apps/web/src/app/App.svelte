<script lang="ts">
  // 현재 화면: 서비스 상태 + 일반 체크리스트 폼 + 공통 UI(대화상자·메뉴) 동작 확인. 기능 화면은 첫 사용자 흐름에서 추가.
  import { fetchHealth } from '$lib/api/client';
  import ChecklistForm from '$lib/features/checklists/ChecklistForm.svelte';
  import type { ChecklistItem } from '$lib/features/checklists/checklist-item';
  import HealthStatus from '$lib/features/health/HealthStatus.svelte';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import * as DropdownMenu from '$lib/ui/dropdown-menu';
  import * as Sidebar from '$lib/ui/sidebar';

  let items = $state<ChecklistItem[]>([]);
  let dialogOpen = $state(false);
  let lastMenuAction = $state<string | undefined>(undefined);
</script>

<Sidebar.Provider>
  <Sidebar.Root>
    <Sidebar.Header>
      <span class="px-2 text-base font-semibold">fvoci</span>
    </Sidebar.Header>
    <Sidebar.Content>
      <Sidebar.Group>
        <Sidebar.GroupLabel>워크스페이스</Sidebar.GroupLabel>
        <Sidebar.GroupContent>
          <Sidebar.Menu>
            <Sidebar.MenuItem>
              <Sidebar.MenuButton isActive>
                {#snippet child({ props })}
                  <a href="/" {...props}>홈</a>
                {/snippet}
              </Sidebar.MenuButton>
            </Sidebar.MenuItem>
          </Sidebar.Menu>
        </Sidebar.GroupContent>
      </Sidebar.Group>
    </Sidebar.Content>
  </Sidebar.Root>

  <Sidebar.Inset>
    <header class="flex items-center gap-2 border-b px-4 py-2">
      <Sidebar.Trigger />
      <h1 class="text-lg font-semibold">홈</h1>
      <div class="ml-auto flex items-center gap-2">
        <DropdownMenu.Root>
          <DropdownMenu.Trigger>
            {#snippet child({ props })}
              <Button variant="outline" {...props}>메뉴</Button>
            {/snippet}
          </DropdownMenu.Trigger>
          <DropdownMenu.Content align="end">
            <DropdownMenu.Item
              onSelect={() => {
                lastMenuAction = '새 문서';
              }}>새 문서</DropdownMenu.Item
            >
            <DropdownMenu.Item
              onSelect={() => {
                lastMenuAction = '새 업무';
              }}>새 업무</DropdownMenu.Item
            >
          </DropdownMenu.Content>
        </DropdownMenu.Root>
        <Button
          onclick={() => {
            dialogOpen = true;
          }}>정보</Button
        >
      </div>
    </header>

    <main class="mx-auto flex w-full max-w-3xl flex-col gap-8 px-4 py-6">
      <HealthStatus load={fetchHealth} />
      {#if lastMenuAction !== undefined}
        <p class="text-sm text-muted-foreground" data-testid="menu-result">
          선택한 메뉴: {lastMenuAction}
        </p>
      {/if}
      <ChecklistForm
        {items}
        onchange={(next: ChecklistItem[]) => {
          items = next;
        }}
      />
    </main>
  </Sidebar.Inset>
</Sidebar.Provider>

<Dialog.Root bind:open={dialogOpen}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>fvoci</Dialog.Title>
      <Dialog.Description>
        통합 작업·지식·학습 관리 앱. 초기화 단계이며 기능 화면은 아직 없습니다.
      </Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Dialog.Close>
        {#snippet child({ props })}
          <Button {...props}>닫기</Button>
        {/snippet}
      </Dialog.Close>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
