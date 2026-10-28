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
// 그래서 파일 이름순으로 읽어 함수마다 마지막 정의의 본문에서만 찾는다(R5 delta2 D2-F-4). 같은 함수는 PostgreSQL이
// 보는 서명으로 묶는다: 이름은 소문자에 `fvoci.` 접두어를 떼고(함수가 `search_path`를 쓰므로 스키마 없이 다시 정의하기
// 쉽다), 인자는 이름·모드·기본값을 빼고 형만(OUT 인자는 서명에 들지 않는다), 흔한 형 동의어는 하나로. `DROP
// FUNCTION`은 그 정의를 지운다(R5 delta3 B D3B-F-1).

interface MigrationFile {
  name: string;
  text: string;
}

/** 형 동의어를 하나로(여러 낱말 형은 먼저 한 낱말로 묶어 인자 이름과 가른다). */
const TYPE_SYNONYMS: readonly [RegExp, string][] = [
  [/\btimestamp\s+with\s+time\s+zone\b/g, 'timestamptz'],
  [/\btimestamp\s+without\s+time\s+zone\b/g, 'timestamp'],
  [/\btime\s+with\s+time\s+zone\b/g, 'timetz'],
  [/\btime\s+without\s+time\s+zone\b/g, 'time'],
  [/\bdouble\s+precision\b/g, 'float8'],
  [/\bcharacter\s+varying\b/g, 'varchar'],
  [/\binteger\b|\bint4\b|\bint\b/g, 'int4'],
  [/\bbigint\b/g, 'int8'],
  [/\bsmallint\b/g, 'int2'],
  [/\bboolean\b/g, 'bool'],
  [/\breal\b/g, 'float4'],
  [/\bdecimal\b/g, 'numeric'],
];

const unqualified = (name: string): string =>
  name
    .replace(/"/g, '')
    .toLowerCase()
    .replace(/^fvoci\./, '');

/** 괄호 안 인자 목록에서 형만 남긴 서명. 기본값 안의 쉼표·괄호는 건너뛴다. */
function signature(args: string): string {
  const parts: string[] = [];
  let depth = 0;
  let current = '';
  for (const ch of args) {
    if (ch === '(') depth += 1;
    if (ch === ')') depth -= 1;
    if (ch === ',' && depth === 0) {
      parts.push(current);
      current = '';
    } else current += ch;
  }
  parts.push(current);
  return parts
    .map((part) => {
      let arg = part
        .toLowerCase()
        .replace(/\s+(default\b|=)[\s\S]*$/, '')
        .trim();
      for (const [pattern, canonical] of TYPE_SYNONYMS) arg = arg.replace(pattern, canonical);
      let tokens = arg.split(/\s+/).filter((t) => t !== '');
      const mode = tokens[0];
      if (mode === 'out') return undefined;
      if (mode === 'in' || mode === 'inout' || mode === 'variadic') tokens = tokens.slice(1);
      // 남은 낱말이 둘 이상이면 첫 낱말은 인자 이름이다(여러 낱말 형은 위에서 한 낱말로 묶었다).
      const type = tokens.length > 1 ? tokens.slice(1).join(' ') : (tokens[0] ?? '');
      return type.replace(/^"?fvoci"?\./, '');
    })
    .filter((type): type is string => type !== undefined && type !== '')
    .join(',');
}

/** `(`에서 짝이 맞는 `)`까지의 안쪽과 그 뒤 위치. */
function parenthesized(text: string, open: number): { inner: string; end: number } | undefined {
  let depth = 0;
  for (let i = open; i < text.length; i += 1) {
    if (text[i] === '(') depth += 1;
    if (text[i] === ')') {
      depth -= 1;
      if (depth === 0) return { inner: text.slice(open + 1, i), end: i + 1 };
    }
  }
  return undefined;
}

const STATEMENT =
  /\b(CREATE\s+(?:OR\s+REPLACE\s+)?FUNCTION|DROP\s+FUNCTION(?:\s+IF\s+EXISTS)?)\s+/gi;
const NAME = /^("?[\w]+"?(?:\."?[\w]+"?)?)\s*/;

/** 함수(정규화한 이름과 인자 형)마다 파일 이름순 마지막 정의의 본문. 지운 함수는 없다. */
function currentFunctions(files: readonly MigrationFile[]): Map<string, string> {
  const bodies = new Map<string, string>();
  for (const file of [...files].sort((a, b) => a.name.localeCompare(b.name))) {
    const text = file.text;
    for (const match of text.matchAll(STATEMENT)) {
      const verb = (match[1] ?? '').toUpperCase();
      let at = match.index + match[0].length;
      if (verb.startsWith('DROP')) {
        // DROP FUNCTION a(...), b, ... [CASCADE | RESTRICT];
        const statement = text.slice(at, text.indexOf(';', at));
        let rest = statement;
        while (rest.trim() !== '') {
          const name = NAME.exec(rest.trimStart());
          if (name === null) break;
          rest = rest.trimStart().slice(name[0].length);
          const fn = unqualified(name[1] ?? '');
          if (rest.startsWith('(')) {
            const args = parenthesized(rest, 0);
            if (args === undefined) break;
            bodies.delete(`${fn}(${signature(args.inner)})`);
            rest = rest.slice(args.end);
          } else {
            for (const key of [...bodies.keys()]) if (key.startsWith(`${fn}(`)) bodies.delete(key);
          }
          const next = rest.replace(/^\s*,/, '');
          if (next === rest) break;
          rest = next;
        }
        continue;
      }
      const name = NAME.exec(text.slice(at));
      if (name === null) continue;
      at += name[0].length;
      const args = parenthesized(text, at);
      if (args === undefined) continue;
      const tag = /\bAS\s+(\$[a-z_]*\$)/i.exec(text.slice(args.end));
      if (tag === null) continue;
      const start = args.end + tag.index + tag[0].length;
      const end = text.indexOf(tag[1] ?? '$$', start);
      if (end === -1) continue;
      bodies.set(`${unqualified(name[1] ?? '')}(${signature(args.inner)})`, text.slice(start, end));
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

const dir = path.resolve(import.meta.dirname, '../../../../crates/db/migrations');
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
    expect(raisedBy(replaced, 'renamed', '55000')).toEqual(['app_x(uuid)']);
  });

  const initial = (message: string): MigrationFile => ({
    name: '202610250001_initial_state.sql',
    text: `CREATE FUNCTION fvoci.app_y(p_id uuid, p_at timestamp with time zone DEFAULT now(), OUT ok boolean) RETURNS boolean\n    LANGUAGE plpgsql\n    AS $_$\n  BEGIN\n    ${raise(message, '55000')};\n  END\n  $_$;\n`,
  });

  it('treats a schema-less redefinition with other spelling as the same function', () => {
    const later: MigrationFile = {
      name: '202701010001_change.sql',
      text: `create or replace function APP_Y(id UUID, at timestamptz) returns boolean\n    language plpgsql\n    as $$\n  begin\n    ${raise('renamed', '55000')};\n  end\n  $$;\n`,
    };
    const bodies = currentFunctions([later, initial('the item is archived')]);
    expect([...bodies.keys()]).toEqual(['app_y(uuid,timestamptz)']);
    expect(raisedBy(bodies, 'the item is archived', '55000')).toEqual([]);
    expect(raisedBy(bodies, 'renamed', '55000')).toEqual(['app_y(uuid,timestamptz)']);
  });

  it('forgets a dropped function, with or without its argument list', () => {
    for (const drop of [
      'DROP FUNCTION fvoci.app_y(uuid, timestamptz);',
      'DROP FUNCTION IF EXISTS app_y(p_id uuid, p_at timestamp with time zone) CASCADE;',
      'DROP FUNCTION app_y;',
    ]) {
      const bodies = currentFunctions([
        { name: '202701010001_drop.sql', text: drop },
        initial('the item is archived'),
      ]);
      expect(raisedBy(bodies, 'the item is archived', '55000')).toEqual([]);
    }
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
