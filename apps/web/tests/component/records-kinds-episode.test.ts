import { afterEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';

// 2단계 서버는 강의 모듈 기록 종류에 `episode`(회차) 칸을 준다. `GET /api/record-kinds`는 모든 모듈의 종류를 한 배열로 주므로
// 웹 파서가 이 칸 형식을 모르면 프로젝트·연구 항목의 기록 화면까지 종류 목록을 읽지 못한다(CodeRabbit #140 591de1c3 지적).
afterEach(() => {
  vi.unstubAllGlobals();
});

const kinds = [
  {
    kind: 'question',
    label: '질문',
    module_kind: 'course',
    date_label: null,
    fields: [
      { key: 'episode', label: '회차', type: 'episode', choices: [] },
      { key: 'body', label: '내용', type: 'long_text', choices: [] },
    ],
    file_roles: [],
  },
  {
    kind: 'meeting',
    label: '회의',
    module_kind: 'project',
    date_label: '회의일',
    fields: [{ key: 'notes', label: '메모', type: 'long_text', choices: [] }],
    file_roles: [],
  },
];

function jsonResponse(body: unknown): Promise<Response> {
  return Promise.resolve(
    new Response(JSON.stringify(body), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    }),
  );
}

describe('record kinds with an episode field', () => {
  it('parses the whole list instead of failing on the course kinds', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() => jsonResponse(kinds)),
    );
    const parsed = await api.recordKinds();
    expect(parsed.map((k) => k.kind)).toEqual(['question', 'meeting']);
    expect(parsed[0]?.fields[0]?.type).toBe('episode');
  });
});
