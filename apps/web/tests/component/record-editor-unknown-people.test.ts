import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import type { RecordKindView } from '$lib/api/client';
import RecordEditor from '$lib/features/records/RecordEditor.svelte';
import { emptyForm } from '$lib/features/records/records';
import { setLocale } from '$lib/paraglide/runtime.js';

// 감사 W15: 사람 칸의 체크는 지금 멤버만 그려, 나간 멤버가 저장된 기록에서 그 사람을 뺄 수 없었다. 저장된 값에 있는
// 멤버가 아닌 사람도 "알 수 없는 사람"으로 보여 뺄 수 있다.
const meeting: RecordKindView = {
  kind: 'meeting',
  label: '모임',
  module_kind: 'study',
  date_label: '모임 날짜',
  fields: [{ key: 'attendees', label: '출석', type: 'people', choices: [] }],
  file_roles: [],
};

describe('record editor people who left', () => {
  it('shows a stored person who is no longer a member and lets the editor remove them', async () => {
    await setLocale('ko', { reload: false });
    const form = { ...emptyForm(), title: '1회', people: { attendees: ['u-alice', 'u-gone'] } };
    const submit = vi.fn();
    render(RecordEditor, {
      props: {
        def: meeting,
        form,
        people: [{ id: 'u-alice', name: '앨리스' }],
        courses: [],
        files: [],
        submitLabel: '저장',
        onsubmit: submit,
        oncancel: () => undefined,
      },
    });

    const gone = screen.getByLabelText('알 수 없는 사람');
    expect(gone).toHaveAttribute('aria-checked', 'true');
    await fireEvent.click(gone);
    // 부모는 폼을 bind로 넘긴다. 여기서는 폼 값에서 빠졌는지 본다.
    expect(form.people.attendees).toEqual(['u-alice']);
    expect(screen.getByLabelText('앨리스')).toHaveAttribute('aria-checked', 'true');
  });
});
