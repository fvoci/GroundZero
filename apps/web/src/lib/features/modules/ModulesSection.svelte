<script lang="ts">
  // 워크스페이스의 모듈 목록과 모듈 만들기(관리자). 같은 유형을 여러 개 둘 수 있다 (D-15).
  import type { Module, ModuleKind, Workspace } from '@fvoci/api-client';

  import { api, ApiError, MODULE_KINDS, NetworkError, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { modulePath, router } from '$lib/router.svelte';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import { asKind, kindLabel, normalizeKey } from './kinds';

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
        if (!cancelled) loadError = errorText(e, m.modules_section_load_failed());
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
      formError = m.modules_name_required();
      return;
    }
    if (normalized === undefined) {
      formError = m.modules_key_invalid();
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
      if (e instanceof NetworkError) formError = m.modules_create_network();
      else if (e instanceof ApiError && e.status === 409) formError = m.modules_key_taken_new();
      else formError = errorText(e, m.modules_create_failed());
    } finally {
      busy = false;
    }
  }
</script>

<section class="flex flex-col gap-3" aria-labelledby="modules-heading">
  <h2 id="modules-heading" class="text-lg font-semibold">{m.modules_heading()}</h2>
  {#if loadError !== undefined}
    <p role="alert" class="text-destructive">{loadError}</p>
  {:else if modules === undefined}
    <p aria-live="polite">{m.modules_loading()}</p>
  {:else if modules.length === 0}
    <p class="text-muted-foreground">
      {m.modules_empty()}{isAdmin ? ` ${m.modules_empty_admin_hint()}` : ''}
    </p>
  {:else}
    <ul class="flex flex-col gap-1" data-testid="module-list">
      {#each modules as mod (mod.id)}
        <li>
          <a
            class="flex items-baseline gap-2 rounded px-2 py-1.5 hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            href={modulePath(workspace.id, mod.id)}
            onclick={(e) => {
              e.preventDefault();
              router.navigate(modulePath(workspace.id, mod.id));
            }}
          >
            <span class="font-mono text-sm text-muted-foreground">{mod.key}</span>
            <span>{mod.name}</span>
            <Badge variant="outline">{kindLabel(mod.kind)}</Badge>
            {#if mod.archived}<Badge variant="secondary">{m.modules_archived()}</Badge>{/if}
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
      aria-label={m.modules_create()}
    >
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium" id="module-kind-label">{m.modules_kind()}</span>
        <Select.Root
          type="single"
          value={kind}
          onValueChange={(v: string) => {
            kind = asKind(v) ?? kind;
          }}
        >
          <Select.Trigger class="w-28" aria-labelledby="module-kind-label"
            >{kindLabel(kind)}</Select.Trigger
          >
          <Select.Content>
            {#each MODULE_KINDS as k (k)}
              <Select.Item value={k} label={kindLabel(k)}>{kindLabel(k)}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex min-w-40 flex-1 flex-col gap-1">
        <Label for="module-name">{m.modules_name()}</Label>
        <Input
          id="module-name"
          bind:value={name}
          placeholder={m.modules_name_placeholder()}
          maxlength={200}
        />
      </div>
      <div class="flex w-28 flex-col gap-1">
        <Label for="module-key">{m.modules_key()}</Label>
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
      <Button type="submit" disabled={busy}>{m.modules_create()}</Button>
    </form>
    {#if formError !== undefined}
      <p id="module-form-error" class="text-sm text-destructive" role="alert">{formError}</p>
    {/if}
  {/if}
</section>
