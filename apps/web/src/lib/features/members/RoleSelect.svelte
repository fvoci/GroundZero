<script lang="ts">
  // 역할 선택. 공통 Select를 그대로 쓰고 표시 이름만 붙인다.
  import type { Role } from '@fvoci/api-client';

  import * as Select from '$lib/ui/select';

  import { asRole, roleLabel, ROLES } from './roles';

  interface Props {
    value: Role;
    label: string;
    disabled?: boolean;
    onchange: (role: Role) => void;
  }

  const { value, label, disabled = false, onchange }: Props = $props();
</script>

<Select.Root
  type="single"
  {value}
  {disabled}
  onValueChange={(next: string) => {
    const role = asRole(next);
    if (role !== undefined && role !== value) onchange(role);
  }}
>
  <Select.Trigger class="w-28" aria-label={label}>{roleLabel(value)}</Select.Trigger>
  <Select.Content>
    {#each ROLES as role (role)}
      <Select.Item value={role} label={roleLabel(role)}>{roleLabel(role)}</Select.Item>
    {/each}
  </Select.Content>
</Select.Root>
