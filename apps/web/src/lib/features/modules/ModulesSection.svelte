<script lang="ts">
  // 워크스페이스의 모듈 목록과 모듈 만들기(관리자). 같은 유형을 여러 개 둘 수 있다 (D-15).
  import type { Module, ModuleKind, Workspace } from '@fvoci/api-client';

  import { api, ApiError, MODULE_KINDS, NetworkError, newRequestId } from '$lib/api/client';
  import { modulePath, router } from '$lib/router.svelte';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import { asKind, KIND_LABEL, normalizeKey } from './kinds';

  interface Props {
    workspace: Workspace;
  }

  const { workspace }: Props = $props();

  let modules = $state<Module[] | undefined>(undefined);
  let loadError = $state<string | undefined>(undefined);

  $effect(() => {
    const wsId = workspace.id;
    let cancelled = false;
    modules = undefined;
    loadError = undefined;
    api
      .modules(wsId)
      .then((list) => {
        if (!cancelled) modules = list;
      })
      .catch((e: unknown) => {
        if (!cancelled)
          loadError = e instanceof ApiError ? e.message : '모듈을 불러오지 못했습니다.';
      });
    return () => {
      cancelled = true;
    };
  });

  const isAdmin = $derived(workspace.role === 'admin');

  let kind = $state<ModuleKind>('project');
  let name = $state('');
  let key = $state('');
  let busy = $state(false);
  let formError = $state<string | undefined>(undefined);
  let pending: { requestId: string; signature: string } | undefined;

  async function create(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const trimmed = name.trim();
    const normalized = normalizeKey(key);
    if (trimmed === '') {
      formError = '이름을 입력하세요.';
      return;
    }
    if (normalized === undefined) {
      formError = '키는 영문으로 시작하는 2~10자 영문·숫자입니다. WIKI는 쓸 수 없습니다.';
      return;
    }
    const signature = `${kind}|${trimmed}|${normalized}`;
    if (pending?.signature !== signature) pending = { requestId: newRequestId(), signature };
    busy = true;
    formError = undefined;
    try {
      const created = await api.createModule(workspace.id, {
        requestId: pending.requestId,
        kind,
        name: trimmed,
        key: normalized,
      });
      pending = undefined;
      name = '';
      key = '';
      router.navigate(modulePath(workspace.id, created.id));
    } catch (e: unknown) {
      if (e instanceof NetworkError)
        formError = '서버에 연결할 수 없습니다. 다시 시도하면 중복 없이 처리됩니다.';
      else if (e instanceof ApiError && e.status === 409)
        formError = '이미 쓰고 있거나 예전에 쓴 키입니다. 다른 키를 고르세요.';
      else if (e instanceof ApiError) formError = e.message;
      else formError = '만들지 못했습니다.';
    } finally {
      busy = false;
    }
  }
</script>

<section class="flex flex-col gap-3" aria-labelledby="modules-heading">
  <h2 id="modules-heading" class="text-lg font-semibold">모듈</h2>
  {#if loadError !== undefined}
    <p role="alert" class="text-destructive">{loadError}</p>
  {:else if modules === undefined}
    <p aria-live="polite">불러오는 중…</p>
  {:else if modules.length === 0}
    <p class="text-muted-foreground">
      모듈이 없습니다.{isAdmin ? ' 아래에서 프로젝트·연구·스터디·강좌·취미 모듈을 만드세요.' : ''}
    </p>
  {:else}
    <ul class="flex flex-col gap-1" data-testid="module-list">
      {#each modules as m (m.id)}
        <li>
          <a
            class="flex items-baseline gap-2 rounded px-2 py-1.5 hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            href={modulePath(workspace.id, m.id)}
            onclick={(e) => {
              e.preventDefault();
              router.navigate(modulePath(workspace.id, m.id));
            }}
          >
            <span class="font-mono text-sm text-muted-foreground">{m.key}</span>
            <span>{m.name}</span>
            <Badge variant="outline">{KIND_LABEL[m.kind]}</Badge>
            {#if m.archived}<Badge variant="secondary">보관됨</Badge>{/if}
          </a>
        </li>
      {/each}
    </ul>
  {/if}

  {#if isAdmin}
    <form
      class="flex flex-wrap items-end gap-2"
      onsubmit={(e) => void create(e)}
      novalidate
      aria-label="모듈 만들기"
    >
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium" id="module-kind-label">유형</span>
        <Select.Root
          type="single"
          value={kind}
          onValueChange={(v: string) => {
            kind = asKind(v) ?? kind;
          }}
        >
          <Select.Trigger class="w-28" aria-labelledby="module-kind-label"
            >{KIND_LABEL[kind]}</Select.Trigger
          >
          <Select.Content>
            {#each MODULE_KINDS as k (k)}
              <Select.Item value={k} label={KIND_LABEL[k]}>{KIND_LABEL[k]}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex min-w-40 flex-1 flex-col gap-1">
        <Label for="module-name">이름</Label>
        <Input id="module-name" bind:value={name} placeholder="예: 개발" maxlength={200} />
      </div>
      <div class="flex w-28 flex-col gap-1">
        <Label for="module-key">키</Label>
        <Input
          id="module-key"
          bind:value={key}
          placeholder="DEV"
          maxlength={10}
          autocomplete="off"
          class="font-mono uppercase"
          aria-invalid={formError !== undefined}
          aria-describedby={formError !== undefined ? 'module-form-error' : undefined}
        />
      </div>
      <Button type="submit" disabled={busy}>모듈 만들기</Button>
    </form>
    {#if formError !== undefined}
      <p id="module-form-error" class="text-sm text-destructive" role="alert">{formError}</p>
    {/if}
  {/if}
</section>
