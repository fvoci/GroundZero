<script lang="ts">
  // 역할 선택. 공통 Select를 그대로 쓰고 표시 이름만 붙인다.
  import type { Role } from '@fvoci/api-client';

  import * as Select from '$lib/ui/select';

  import { asRole, roleLabel, ROLES } from './roles';

  interface Props {
    value: Role;
    label: string;
    disabled?: boolean;
    roles?: readonly Role[];
    describedBy?: string;
    invalid?: boolean;
    onchange: (role: Role) => void;
  }

  const {
    value,
    label,
    disabled = false,
    roles = ROLES,
    describedBy,
    invalid = false,
    onchange,
  }: Props = $props();
</script>

<Select.Root
  type="single"
  {value}
  {disabled}
  onValueChange={(next: string) => {
    const role = asRole(next);
    if (role !== undefined && roles.includes(role) && role !== value) onchange(role);
  }}
>
  <!-- 접근성 이름에 지금 역할을 함께 넣는다. aria-label이 보이는 글을 대신하므로 칸 이름만 두면 화면 낭독기가 지금
       값을 읽지 않고, 음성 입력은 보이는 글(역할 이름)로 찾지 못한다(WCAG 2.5.3). -->
  <Select.Trigger
    class="w-28"
    aria-label={`${label}: ${roleLabel(value)}`}
    aria-describedby={describedBy}
    aria-invalid={invalid}>{roleLabel(value)}</Select.Trigger
  >
  <Select.Content>
    {#each roles as role (role)}
      <Select.Item value={role} label={roleLabel(role)}>{roleLabel(role)}</Select.Item>
    {/each}
  </Select.Content>
</Select.Root>
