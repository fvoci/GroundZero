<script lang="ts">
  import type { Organization } from '@fvoci/api-client';
  import { onMount, untrack } from 'svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { ApiError } from '$lib/shared/api';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { NativeSelect } from '$lib/ui/native-select';
  import * as Switch from '$lib/ui/switch';
  import { organizationsApi } from './api';
  import { organizationError, organizationUnavailable } from './feedback';
  import {
    bytesPerUnit,
    isByteLimit,
    isRetention,
    LIMIT_KEYS,
    limitInputs,
    limitValue,
    type LimitKey,
  } from './limits';
  import type { OrganizationSettingsPatch } from './model';

  const {
    organization,
    onchanged,
    onunavailable,
  }: {
    organization: Organization;
    onchanged: (organization: Organization) => void;
    onunavailable: () => void;
  } = $props();
  let baseline = $state(untrack(() => organization));
  let name = $state(untrack(() => organization.name));
  let publicLinks = $state(untrack(() => organization.public_links_enabled));
  let limits = $state(untrack(() => limitInputs(organization.limits)));
  let error = $state('');
  let fieldErrors = $state<Record<string, string>>({});
  let busy = $state(false);
  let confirmLinks = $state(false);
  const dirty = $derived(
    name !== baseline.name ||
      publicLinks !== baseline.public_links_enabled ||
      LIMIT_KEYS.some((key) => limitValue(key, limits[key]) !== (baseline.limits[key] ?? null)),
  );
  const draft = createFormDraft({
    preserve: false,
    name: () => `organization-settings:${organization.id}`,
    snapshot: () => ({ name, publicLinks, limits }),
    parse: () => undefined,
    restore: () => undefined,
    reset: () => {
      name = baseline.name;
      publicLinks = baseline.public_links_enabled;
      limits = limitInputs(baseline.limits);
    },
    dirty: () => dirty,
    busy: () => busy,
  });
  onMount(() => draft.attach());

  function label(key: LimitKey): string {
    switch (key) {
      case 'max_file_bytes':
        return m.organizations_max_file();
      case 'max_storage_bytes':
        return m.organizations_max_storage();
      case 'max_concurrent_uploads':
        return m.organizations_max_uploads();
      case 'activity_retention_days':
        return m.organizations_activity_retention();
      case 'audit_retention_days':
        return m.organizations_audit_retention();
    }
  }
  function changeUnit(key: LimitKey, event: Event): void {
    const target = event.currentTarget;
    if (!(target instanceof HTMLSelectElement) || (target.value !== 'MB' && target.value !== 'GB'))
      return;
    const current = limits[key];
    const numeric = Number(current.amount);
    limits[key] = {
      ...current,
      amount:
        current.amount.trim() !== '' && Number.isFinite(numeric)
          ? String((numeric * bytesPerUnit(current.unit)) / bytesPerUnit(target.value))
          : current.amount,
      unit: target.value,
    };
  }
  function changeLinks(checked: boolean): void {
    if (checked) publicLinks = true;
    else confirmLinks = true;
  }
  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (busy) return;
    const patch: OrganizationSettingsPatch = {};
    fieldErrors = {};
    error = '';
    if (name.trim() === '') fieldErrors.name = m.organizations_name_required();
    else if (name.trim() !== baseline.name) patch.name = name.trim();
    if (publicLinks !== baseline.public_links_enabled) patch.public_links_enabled = publicLinks;
    for (const key of LIMIT_KEYS) {
      const value = limitValue(key, limits[key]);
      if (value === undefined)
        fieldErrors[key] = isRetention(key)
          ? m.organizations_retention_invalid()
          : m.organizations_limit_invalid();
      else if (value !== (baseline.limits[key] ?? null)) patch[key] = value;
    }
    if (Object.keys(fieldErrors).length > 0 || Object.keys(patch).length === 0) return;
    const attempt = draft.capture();
    if (attempt === undefined) return;
    busy = true;
    try {
      const saved = await organizationsApi.settings(organization.id, patch);
      if (!draft.isCurrent(attempt)) return;
      baseline = saved;
      name = saved.name;
      publicLinks = saved.public_links_enabled;
      limits = limitInputs(saved.limits);
      notifyDone(m.common_saved());
      onchanged(saved);
    } catch (e: unknown) {
      if (!draft.isCurrent(attempt)) return;
      if (organizationUnavailable(e)) onunavailable();
      else if (
        e instanceof ApiError &&
        e.status === 400 &&
        e.field !== undefined &&
        (e.field === 'name' ||
          e.field === 'public_links_enabled' ||
          LIMIT_KEYS.some((key) => key === e.field))
      )
        fieldErrors[e.field] = organizationError(e);
      else error = organizationError(e);
    } finally {
      if (draft.isOwner(attempt)) busy = false;
    }
  }
</script>

<form class="flex min-w-0 max-w-2xl flex-col gap-6" onsubmit={(event) => void save(event)}>
  <div class="space-y-2">
    <Label for="organization-name">{m.organizations_name()}</Label>
    <Input
      id="organization-name"
      bind:value={name}
      disabled={busy}
      aria-invalid={Boolean(fieldErrors.name)}
      aria-describedby="organization-name-error"
    />
    <p id="organization-name-error" class="text-sm text-destructive empty:hidden">
      {fieldErrors.name ?? ''}
    </p>
  </div>
  <div class="space-y-2">
    <div class="flex items-center gap-3">
      <Switch.Root
        id="organization-public-links"
        checked={publicLinks}
        onCheckedChange={changeLinks}
        disabled={busy}
        aria-describedby="organization-links-error"
        class="inline-flex h-6 w-10 shrink-0 items-center rounded-full border border-input-border bg-muted px-0.5 outline-none focus-visible:ring-2 focus-visible:ring-ring data-checked:bg-primary"
      >
        <Switch.Thumb
          class="block size-4 rounded-full bg-background transition-transform data-checked:translate-x-4"
        />
      </Switch.Root>
      <Label for="organization-public-links">{m.organizations_public_links()}</Label>
    </div>
    <p id="organization-links-error" class="text-sm text-destructive empty:hidden">
      {fieldErrors.public_links_enabled ?? ''}
    </p>
  </div>
  <fieldset class="min-w-0 space-y-5" disabled={busy}>
    <legend class="mb-3 font-semibold">{m.organizations_limits()}</legend>
    {#each LIMIT_KEYS as key (key)}
      <div class="space-y-2">
        <Label for={`organization-limit-${key}`}>{label(key)}</Label>
        <div class="flex min-w-0 flex-wrap items-center gap-3">
          <div class="min-w-0 flex-1 basis-40">
            <Input
              id={`organization-limit-${key}`}
              inputmode={isByteLimit(key) ? 'decimal' : 'numeric'}
              bind:value={limits[key].amount}
              disabled={limits[key].unset || busy}
              aria-invalid={Boolean(fieldErrors[key])}
              aria-describedby={`organization-limit-${key}-error organization-limit-${key}-hint`}
            />
          </div>
          {#if isByteLimit(key)}<NativeSelect
              class="shrink-0"
              aria-label={m.organizations_unit({ label: label(key) })}
              value={limits[key].unit}
              disabled={limits[key].unset || busy}
              onchange={(event: Event) => {
                changeUnit(key, event);
              }}><option value="MB">MB</option><option value="GB">GB</option></NativeSelect
            >{/if}
          <div class="flex items-center gap-2">
            <Checkbox
              id={`organization-limit-${key}-unset`}
              bind:checked={limits[key].unset}
              disabled={busy}
              aria-label={m.organizations_unset_named({ label: label(key) })}
            /><Label for={`organization-limit-${key}-unset`}>{m.organizations_unset()}</Label>
          </div>
        </div>
        <p id={`organization-limit-${key}-hint`} class="text-sm text-muted-foreground">
          {limits[key].unset ? m.organizations_default_hint() : ''}{isRetention(key)
            ? ` ${m.organizations_retention_hint()}`
            : ''}
        </p>
        <p id={`organization-limit-${key}-error`} class="text-sm text-destructive empty:hidden">
          {fieldErrors[key] ?? ''}
        </p>
      </div>
    {/each}
  </fieldset>
  <p role="alert" class="text-sm text-destructive empty:hidden">{error}</p>
  <Button class="self-start" type="submit" disabled={busy || !dirty}>{m.common_save()}</Button>
</form>
<Dialog.Root bind:open={confirmLinks}>
  <Dialog.Content
    class="max-h-[90vh] overflow-y-auto"
    onCloseAutoFocus={(event: Event) => {
      event.preventDefault();
      document.getElementById('organization-public-links')?.focus();
    }}
  >
    <Dialog.Header
      ><Dialog.Title>{m.organizations_disable_links()}</Dialog.Title><Dialog.Description
        >{m.organizations_disable_links_body()}</Dialog.Description
      ></Dialog.Header
    >
    <Dialog.Footer
      ><Button
        variant="ghost"
        onclick={() => {
          confirmLinks = false;
        }}>{m.common_cancel()}</Button
      ><Button
        variant="destructive"
        onclick={() => {
          publicLinks = false;
          confirmLinks = false;
        }}>{m.organizations_disable_links()}</Button
      ></Dialog.Footer
    >
  </Dialog.Content>
</Dialog.Root>
<DiscardChangesConfirm
  open={draft.leave.open}
  onkeep={draft.leave.keep}
  ondiscard={draft.discardAndLeave}
/>
