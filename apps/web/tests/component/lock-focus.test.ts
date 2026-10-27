import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { keepFocusAcrossLocks } from '$lib/features/common/lock-focus';
import StatusEditor from '$lib/features/tasks/StatusEditor.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { TEST_STATUSES } from './fixtures/task-statuses';

// 저장 중 잠금이 풀리면 Enter로 제출한 입력에 초점이 돌아오는지(R5 2차 delta 4묶음 확인 필요 1). 앱은 main.ts에서 한 번
// 켠다. 다른 시험의 초점 단언에 섞이지 않게 setup이 아니라 이 파일에서만 켠다. jsdom에는 브라우저의 focus fixup(비활성이
// 된 요소에서 초점 빼기)이 없고 비활성 요소의 `blur()`는 아무 일도 하지 않아서, 같은 틱에 `blur()`한 뒤 잠가 브라우저가
// 남기는 상태(잠긴 폼, body의 초점)를 만든다. 실제 브라우저는 e2e/save-lock-focus.spec.ts.

let stop: (() => void) | undefined;
beforeEach(() => {
  stop = keepFocusAcrossLocks();
});
afterEach(() => {
  stop?.();
  stop = undefined;
  document.body.replaceChildren();
  vi.restoreAllMocks();
});

const settle = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

function form(): {
  fieldset: HTMLFieldSetElement;
  input: HTMLInputElement;
  other: HTMLButtonElement;
} {
  document.body.innerHTML =
    '<form><fieldset><input id="name"><button type="submit">저장</button></fieldset></form>' +
    '<button id="other" type="button">다른 곳</button>';
  const fieldset = document.querySelector('fieldset');
  const input = document.querySelector<HTMLInputElement>('#name');
  const other = document.querySelector<HTMLButtonElement>('#other');
  if (fieldset === null || input === null || other === null) throw new Error('form');
  return { fieldset, input, other };
}

/** 브라우저처럼 초점을 body로 빼며 잠근다. */
async function lock(fieldset: HTMLFieldSetElement, input: HTMLInputElement): Promise<void> {
  input.blur();
  fieldset.disabled = true;
  await settle();
}

describe('keepFocusAcrossLocks', () => {
  it('returns focus to the input when its fieldset unlocks', async () => {
    const { fieldset, input } = form();
    input.focus();
    await lock(fieldset, input);
    expect(document.activeElement).toBe(document.body);
    fieldset.disabled = false;
    await settle();
    expect(document.activeElement).toBe(input);
  });

  it('waits for an overlapping lock on the same input', async () => {
    const { fieldset, input } = form();
    input.focus();
    input.blur();
    input.disabled = true;
    await lock(fieldset, input);
    fieldset.disabled = false;
    await settle();
    expect(document.activeElement).toBe(document.body);
    input.disabled = false;
    await settle();
    expect(document.activeElement).toBe(input);
  });

  it('does not steal focus the user moved elsewhere during the lock', async () => {
    const { fieldset, input, other } = form();
    input.focus();
    await lock(fieldset, input);
    other.focus();
    fieldset.disabled = false;
    await settle();
    expect(document.activeElement).toBe(other);
  });

  it('does not refocus once the user has focused something else, even if focus is back on body', async () => {
    const { fieldset, input, other } = form();
    input.focus();
    await lock(fieldset, input);
    other.focus();
    other.blur();
    fieldset.disabled = false;
    await settle();
    expect(document.activeElement).toBe(document.body);
  });

  it('does nothing when the input was removed before the unlock', async () => {
    const { fieldset, input } = form();
    input.focus();
    await lock(fieldset, input);
    input.remove();
    fieldset.disabled = false;
    await settle();
    expect(document.activeElement).toBe(document.body);
  });

  it('ignores locks that did not contain the focused element', async () => {
    const { fieldset, input, other } = form();
    other.focus();
    fieldset.disabled = true;
    await settle();
    other.blur();
    fieldset.disabled = false;
    await settle();
    expect(document.activeElement).toBe(document.body);
    expect(document.activeElement).not.toBe(input);
  });

  // D5-F-1(R1): 사용자가 먼저 떠난 입력(빈 곳 클릭, `blur()`, 마우스 클릭으로 단추에 초점을 주지 않는 Safari·macOS
  // Firefox에서 단추 누르기)은 나중 작업(task)에서 잠기고 풀려도 초점을 끌어오지 않는다.
  it('does not pull focus back to an input the user left before the lock', async () => {
    const { fieldset, input } = form();
    input.focus();
    input.blur();
    await new Promise((r) => setTimeout(r, 50));
    fieldset.disabled = true;
    await settle();
    fieldset.disabled = false;
    await settle();
    expect(document.activeElement).toBe(document.body);
  });

  it('stops after the returned cleanup runs', async () => {
    stop?.();
    stop = undefined;
    const { fieldset, input } = form();
    input.focus();
    await lock(fieldset, input);
    fieldset.disabled = false;
    await settle();
    expect(document.activeElement).toBe(document.body);
  });
});

// 저장에 성공하면 닫히는 대화상자: 잠금이 풀릴 때 입력은 이미 사라지거나 닫히는 중이다. 닫힌 대화상자 안으로 초점을
// 되돌리지 않는다.
it('does not focus into a status dialog that closes on success', async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  localStorage.clear();
  const created = TEST_STATUSES[0];
  if (created === undefined) throw new Error('status fixture');
  let finish: (() => void) | undefined;
  vi.spyOn(api, 'createTaskStatus').mockReturnValue(
    new Promise((resolve) => {
      finish = () => {
        resolve(created);
      };
    }),
  );
  const onchanged = vi.fn(() => Promise.resolve());
  render(StatusEditor, { itemId: 'i', statuses: TEST_STATUSES, archived: false, onchanged });
  await fireEvent.click(screen.getByRole('button', { name: m.tasks_status_add() }));
  const name = screen.getByLabelText(m.tasks_status_name());
  name.focus();
  await fireEvent.input(name, { target: { value: '공동 검토' } });
  // 제출과 같은 틱에 잠기며 초점이 body로 빠진다(위 lock()과 같은 흉내).
  name.blur();
  await fireEvent.submit(name.closest('form') ?? name);
  await waitFor(() => {
    expect(name).toBeDisabled();
  });
  // 문서에서 빠진 요소는 activeElement가 될 수 없으므로, 닫히는 동안 한 번이라도 초점을 받았는지 수신기로 본다
  // (e2e/save-lock-focus.spec.ts와 같은 방식).
  const refocused = vi.fn();
  name.addEventListener('focus', refocused);
  finish?.();
  await waitFor(() => {
    expect(screen.queryByRole('dialog')).toBeNull();
  });
  await settle();
  expect(refocused).not.toHaveBeenCalled();
  expect(name.isConnected).toBe(false);
  me.userId = '';
  localStorage.clear();
});
