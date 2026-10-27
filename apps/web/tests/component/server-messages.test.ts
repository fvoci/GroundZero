import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

import { ApiError } from '$lib/api/client';
import {
  ARCHIVED_MESSAGES,
  MATCHED_SERVER_MESSAGES,
  STATUS_NAME_EXISTS_MESSAGE,
  UNTIL_BEFORE_START_MESSAGE,
  isArchivedRejection,
  statusNameExists,
  untilBeforeStart,
} from '$lib/api/errors';

// 계약에 따로 오류 코드가 없는 거절을 웹은 서버 본문의 고정 문구로 판별한다(보관 409 세 문구 R5 P2 F-7, 반복 끝 400
// R5 F-6, 상태 이름 중복 409 R5 delta F-2). 문구는 마이그레이션 SQL의 RAISE EXCEPTION에서 오고 서버 시험은 문구를
// 고정하지 않으므로, 서버가 문구나 SQLSTATE를 바꾸면 웹 판별이 조용히 일반 안내로 떨어지지 않게 여기서 한꺼번에 본다.
// 판별 문구는 모두 `src/lib/api/errors.ts`의 `MATCHED_SERVER_MESSAGES`에 둔다.
//
// 마이그레이션은 덧붙이기만 하므로 뒤 파일이 함수를 `CREATE OR REPLACE`로 바꾸며 문구를 고쳐도 앞 파일의 옛 줄은 남는다.
// 그래서 파일 이름순으로 읽어 함수(이름과 인자)마다 마지막 정의의 본문에서만 찾는다(R5 delta2 D2-F-4).

interface MigrationFile {
  name: string;
  text: string;
}

const FUNCTION =
  /CREATE (?:OR REPLACE )?FUNCTION ([\w.]+)\(([^)]*)\)[\s\S]*?AS (\$[a-z_]*\$)([\s\S]*?)\3/g;

/** 함수(이름과 인자)마다 파일 이름순 마지막 정의의 본문. */
function currentFunctions(files: readonly MigrationFile[]): Map<string, string> {
  const bodies = new Map<string, string>();
  for (const file of [...files].sort((a, b) => a.name.localeCompare(b.name))) {
    for (const match of file.text.matchAll(FUNCTION)) {
      const [, name = '', args = '', , body = ''] = match;
      bodies.set(`${name}(${args.replace(/\s+/g, ' ').trim()})`, body);
    }
  }
  return bodies;
}

const raise = (message: string, sqlstate: string): string =>
  `RAISE EXCEPTION '${message}' USING ERRCODE = '${sqlstate}'`;

/** 지금 정의에서 이 문구를 이 SQLSTATE로 내는 함수들. */
function raisedBy(bodies: Map<string, string>, message: string, sqlstate: string): string[] {
  return [...bodies].filter(([, body]) => body.includes(raise(message, sqlstate))).map(([k]) => k);
}

const dir = path.resolve(import.meta.dirname, '../../../../crates/server/migrations');
const files: MigrationFile[] = readdirSync(dir)
  .filter((name) => name.endsWith('.sql'))
  .map((name) => ({ name, text: readFileSync(path.join(dir, name), 'utf8') }));
const current = currentFunctions(files);

describe('server messages the web matches', () => {
  it('reads the function definitions of the migrations', () => {
    expect(current.size).toBeGreaterThan(100);
  });

  it.each(MATCHED_SERVER_MESSAGES)(
    'finds "$message" raised with SQLSTATE $sqlstate by a current function definition',
    ({ message, sqlstate }) => {
      expect(raisedBy(current, message, sqlstate)).not.toEqual([]);
    },
  );

  it('lists every message a predicate matches', () => {
    expect(MATCHED_SERVER_MESSAGES.map((entry) => entry.message).sort()).toEqual(
      [...ARCHIVED_MESSAGES, STATUS_NAME_EXISTS_MESSAGE, UNTIL_BEFORE_START_MESSAGE].sort(),
    );
  });

  it('covers every archived 55000 error the current definitions raise', () => {
    const raised = new Set(
      [...current.values()].flatMap((body) =>
        [...body.matchAll(/RAISE EXCEPTION '([^']*archived[^']*)' USING ERRCODE = '55000'/g)].map(
          (match) => match[1],
        ),
      ),
    );
    expect([...raised].sort()).toEqual([...ARCHIVED_MESSAGES].sort());
  });

  it('ignores a message that a later migration replaced', () => {
    const define = (message: string): string =>
      `CREATE FUNCTION fvoci.app_x(p_id uuid) RETURNS void\n    LANGUAGE plpgsql\n    AS $$\n  BEGIN\n    ${raise(message, '55000')};\n  END\n  $$;\n`;
    const replaced = currentFunctions([
      {
        name: '202701010001_change.sql',
        text: define('renamed').replace('CREATE FUNCTION', 'CREATE OR REPLACE FUNCTION'),
      },
      { name: '202610250001_initial_state.sql', text: define('the item is archived') },
    ]);
    expect(raisedBy(replaced, 'the item is archived', '55000')).toEqual([]);
    expect(raisedBy(replaced, 'renamed', '55000')).toEqual(['fvoci.app_x(p_id uuid)']);
  });

  it('matches each message only with the status code the server maps its SQLSTATE to', () => {
    for (const message of ARCHIVED_MESSAGES) {
      expect(isArchivedRejection(new ApiError(409, 'invalid_state', message))).toBe(true);
      expect(isArchivedRejection(new ApiError(400, 'bad_request', message))).toBe(false);
    }
    expect(statusNameExists(new ApiError(409, 'invalid_state', STATUS_NAME_EXISTS_MESSAGE))).toBe(
      true,
    );
    expect(statusNameExists(new ApiError(409, 'invalid_state', 'other'))).toBe(false);
    expect(statusNameExists(new Error(STATUS_NAME_EXISTS_MESSAGE))).toBe(false);
    expect(untilBeforeStart(new ApiError(400, 'bad_request', UNTIL_BEFORE_START_MESSAGE))).toBe(
      true,
    );
    expect(untilBeforeStart(new ApiError(400, 'bad_request', 'other'))).toBe(false);
    expect(untilBeforeStart(new Error(UNTIL_BEFORE_START_MESSAGE))).toBe(false);
  });
});
