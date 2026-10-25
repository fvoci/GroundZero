import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import RoleSelect from '$lib/features/members/RoleSelect.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 역할 선택 단추의 접근성 이름은 칸 이름과 지금 역할을 함께 담는다. 칸 이름만 두면 aria-label이 보이는 역할 이름을
// 가려 화면 낭독기가 지금 값을 읽지 않았다(Storybook에서 발견).
describe('role select accessible name', () => {
  it('includes the visible current role', async () => {
    await setLocale('ko', { reload: false });
    render(RoleSelect, {
      props: { value: 'writer', label: '김연구 역할', onchange: () => undefined },
    });
    const trigger = screen.getByRole('button', { name: '김연구 역할: 작성자' });
    expect(trigger).toHaveTextContent('작성자');
  });
});
