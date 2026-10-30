<script lang="ts">
  import type { OrganizationLimits } from '@fvoci/api-client';
  import { onDestroy, onMount, untrack } from 'svelte';

  import { api, ApiError } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { me } from '$lib/me.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { NativeSelect } from '$lib/ui/native-select';
  import { Skeleton } from '$lib/ui/skeleton';

  import { workspaceLimitsApi } from './limits-api';
  import {
    LIMIT_FIELDS,
    limitInputs,
    limitErrors,
    limitLabel,
    limitsPatch,
    organizationCap,
    type LimitKey,
  } from './limits';

  const { workspaceId, organizationId }: { workspaceId: string; organizationId: string } = $props();
  let saved = $state<OrganizationLimits>({});
  let organization = $state<OrganizationLimits>({});
  let inputs = $state(limitInputs({}));
  let baseline = $state(JSON.stringify(limitInputs({})));
  let loading = $state(true);
  let loadError = $state<string>();
  let inaccessible = $state(false);
  let saveError = $state<string>();
  let serverErrors = $state<Partial<Record<LimitKey, string>>>({});
  let submitted = $state(false);
  let saving = $state(false);
  const errors = $derived({ ...serverErrors, ...limitErrors(inputs, organization, submitted) });
  const dirty = $derived(!loading && JSON.stringify(inputs) !== baseline);
  let generation = 0;
  let disposed = false;
  onDestroy(() => {
    disposed = true;
  });

  function reset(): void {
    inputs = limitInputs(saved);
    baseline = JSON.stringify(inputs);
    submitted = false;
    serverErrors = {};
    saveError = undefined;
  }
  const draft = createFormDraft({
    preserve: false,
    name: () => `workspace-limits:${organizationId}:${workspaceId}`,
    snapshot: () => inputs,
    parse: () => undefined,
    restore: () => undefined,
    reset,
    dirty: () => dirty,
    busy: () => saving,
  });
  onMount(() => draft.attach());

  async function load(): Promise<void> {
    const sequence = ++generation;
    const wsId = workspaceId;
    const orgId = organizationId;
    const userId = me.userId;
    const current = (): boolean =>
      !disposed &&
      sequence === generation &&
      workspaceId === wsId &&
      organizationId === orgId &&
      me.userId === userId;
    loading = true;
    loadError = undefined;
    inaccessible = false;
    saving = false;
    try {
      const [values, org] = await Promise.all([
        workspaceLimitsApi.get(wsId),
        api.organization(orgId),
      ]);
      if (!current()) return;
      saved = values;
      organization = org.limits;
      reset();
    } catch (error: unknown) {
      if (!current()) return;
      inaccessible = error instanceof ApiError && (error.status === 403 || error.status === 404);
      loadError = inaccessible
        ? m.organizations_unavailable()
        : errorText(error, m.organizations_limits_load_failed());
    } finally {
      if (current()) loading = false;
    }
  }
  const loadKey = $derived(JSON.stringify([workspaceId, organizationId, me.userId]));
  $effect(() => {
    if (loadKey !== '')
      untrack(() => {
        void load();
      });
  });

  function changed(key: LimitKey): void {
    serverErrors = Object.fromEntries(
      Object.entries(serverErrors).filter(([field]) => field !== key),
    );
  }

  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (saving || loading || loadError !== undefined || !dirty) return;
    submitted = true;
    if (Object.keys(errors).length > 0) return;
    const attempt = draft.capture();
    if (attempt === undefined) return;
    const patch = limitsPatch(inputs, saved);
    if (Object.keys(patch).length === 0) {
      reset();
      return;
    }
    saving = true;
    saveError = undefined;
    try {
      const values = await workspaceLimitsApi.set(workspaceId, patch);
      if (!draft.isCurrent(attempt)) return;
      saved = values;
      reset();
      notifyDone(m.common_saved());
    } catch (error: unknown) {
      if (!draft.isCurrent(attempt)) return;
      // SQL may put the field in the diagnostic message; structured `field` takes precedence.
      const field =
        error instanceof ApiError
          ? LIMIT_FIELDS.find(
              ({ key }) =>
                error.field === key ||
                (error.field === undefined && error.message.startsWith(`${key} `)),
            )?.key
          : undefined;
      if (error instanceof ApiError && error.status === 400 && field !== undefined) {
        serverErrors[field] = error.message.includes('must not exceed the organization value')
          ? m.organizations_limit_exceeds_organization()
          : errorText(error, m.common_error_bad_request());
      } else {
        saveError = errorText(error, m.organizations_limits_save_failed());
      }
    } finally {
      if (draft.isOwner(attempt)) saving = false;
    }
  }
</script>

<section class="flex min-w-0 flex-col gap-3" aria-labelledby="workspace-limits-heading">
  <h2 id="workspace-limits-heading" class="font-semibold">{m.organizations_limits_heading()}</h2>
  <p class="text-sm text-muted-foreground">{m.organizations_limits_effective_hint()}</p>
  {#if loading}
    <div aria-label={m.app_loading()} aria-busy="true" class="grid gap-3">
      {#each LIMIT_FIELDS as field (field.key)}<Skeleton class="h-16 w-full" />{/each}
    </div>
  {:else if loadError !== undefined}
    <div class="flex flex-col items-start gap-2">
      <p role="alert">{loadError}</p>
      {#if inaccessible}
        <Button variant="outline" href="/">{m.organizations_home()}</Button>
      {:else}
        <Button variant="outline" onclick={() => void load()}>{m.common_retry()}</Button>
      {/if}
    </div>
  {:else}
    <form class="flex min-w-0 flex-col gap-3" onsubmit={(event) => void save(event)} novalidate>
      <fieldset
        class="grid min-w-0 gap-4 @2xl:grid-cols-2"
        disabled={saving}
        aria-describedby={saving ? 'workspace-limits-saving' : undefined}
      >
        {#each LIMIT_FIELDS as field (field.key)}
          {@const key = field.key}
          {@const id = `workspace-limit-${key}`}
          {@const label = limitLabel(key)}
          <div class="flex min-w-0 flex-col gap-2">
            <Label for={id}>{label}</Label>
            <NativeSelect
              aria-label={m.organizations_limit_setting({ field: label })}
              bind:value={inputs[key].mode}
              onchange={() => {
                changed(key);
              }}
              class="w-full"
            >
              <option value="unset">{m.organizations_limit_unset()}</option>
              <option value="set">{m.organizations_limit_set()}</option>
            </NativeSelect>
            <div class="flex min-w-0 flex-wrap items-center gap-2">
              <Input
                {id}
                class="min-w-0 flex-1 basis-24"
                inputmode={field.bytes ? 'decimal' : 'numeric'}
                bind:value={inputs[key].value}
                oninput={() => {
                  changed(key);
                }}
                disabled={inputs[key].mode === 'unset'}
                aria-invalid={errors[key] !== undefined}
                aria-describedby={`${id}-cap${inputs[key].mode === 'unset' ? ` ${id}-unset` : ''}${errors[key] === undefined ? '' : ` ${id}-error`}`}
              />
              {#if field.bytes}
                <NativeSelect
                  aria-label={m.organizations_limit_unit({ field: label })}
                  bind:value={inputs[key].unit}
                  onchange={() => {
                    changed(key);
                  }}
                  disabled={inputs[key].mode === 'unset'}
                  aria-describedby={inputs[key].mode === 'unset' ? `${id}-unset` : undefined}
                >
                  <option value="MB">MB</option><option value="GB">GB</option>
                </NativeSelect>
              {/if}
            </div>
            <p id={`${id}-cap`} class="text-sm text-muted-foreground">
              {m.organizations_limit_organization_cap({
                value: organizationCap(key, organization),
              })}
            </p>
            {#if inputs[key].mode === 'unset'}
              <p id={`${id}-unset`} class="text-sm text-muted-foreground">
                {m.organizations_limit_unset_hint()}
              </p>
            {/if}
            <p
              id={`${id}-error`}
              role="alert"
              hidden={errors[key] === undefined}
              class="text-sm text-destructive empty:hidden"
            >
              {errors[key] ?? ''}
            </p>
          </div>
        {/each}
      </fieldset>
      <p
        role="alert"
        hidden={saveError === undefined}
        class="text-sm text-destructive empty:hidden"
      >
        {saveError ?? ''}
      </p>
      <p id="workspace-limits-saving" role="status" class="text-sm empty:hidden">
        {saving ? m.organizations_limits_saving() : ''}
      </p>
      <div>
        <Button
          type="submit"
          disabled={!dirty || saving}
          aria-describedby={saving
            ? 'workspace-limits-saving'
            : !dirty
              ? 'workspace-limits-unchanged'
              : undefined}>{m.common_save()}</Button
        >
      </div>
      <p id="workspace-limits-unchanged" class="text-sm text-muted-foreground empty:hidden">
        {dirty ? '' : m.organizations_limits_unchanged()}
      </p>
    </form>
  {/if}
</section>

<DiscardChangesConfirm
  open={draft.leave.open}
  onkeep={draft.leave.keep}
  ondiscard={draft.discardAndLeave}
/>
