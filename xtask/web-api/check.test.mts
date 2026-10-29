import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { checkUsage, readAllowlist, type Allowlist } from './check.mts';

const shared = 'apps/web/src/lib/shared/api/index.ts';
const source = 'apps/web/src/lib/example.ts';
const contract = { paths: { '/api/tasks/{task_id}': { get: {} } } };
const empty: Allowlist = { forwarders: [], unused: [] };
function files(code: string): Map<string, string> {
  return new Map([
    [shared, 'export async function request(method: string, path: string) {}'],
    [source, `import { request } from '$lib/shared/api';\n${code}`],
  ]);
}

await test('wrong path fails with the source location and operation', () => {
  assert.throws(
    () => checkUsage(files("request('GET', '/api/typo/42');"), contract, empty),
    /example.ts:2:.*GET \/api\/typo\/42.*not in OpenAPI/,
  );
});

await test('wrong method fails even when the path exists', () => {
  assert.throws(
    () => checkUsage(files("request('DELETE', '/api/tasks/42');"), contract, empty),
    /DELETE \/api\/tasks\/42.*not in OpenAPI/,
  );
});

await test('encoded path segments and query expressions resolve without inspecting query values', () => {
  const result = checkUsage(
    files(`
    function load(id: string, params: unknown) {
      return request('GET', \`/api/tasks/\${encodeURIComponent(id)}?\${params}\`);
    }
  `),
    contract,
    empty,
  );
  assert.equal(result.calls, 1);
  assert.deepEqual(result.used, ['GET /api/tasks/{task_id}']);
});

await test('numeric String conversions are single segments', () => {
  const result = checkUsage(
    files(`
    function load(revision: number) { return request('GET', \`/api/tasks/\${String(revision)}\`); }
  `),
    contract,
    empty,
  );
  assert.equal(result.calls, 1);
});

await test('conditional resources, const bases and optional query suffixes expand every branch', () => {
  const result = checkUsage(
    files(`
    function load(kind: string, id: string, before?: string) {
      const base = \`/api/\${kind === 'task' ? 'tasks' : 'documents'}/\${encodeURIComponent(id)}\`;
      const query = before === undefined ? '' : \`?before=\${encodeURIComponent(before)}\`;
      return request('GET', \`\${base}\${query}\`);
    }
  `),
    { paths: { '/api/tasks/{id}': { get: {} }, '/api/documents/{id}': { get: {} } } },
    empty,
  );
  assert.equal(result.used.length, 2);
});

await test('literal union path segments expand to every possible route', () => {
  const result = checkUsage(
    files(`
    function load(kind: 'tasks' | 'documents') { request('GET', \`/api/\${kind}/42\`); }
  `),
    { paths: { '/api/tasks/{id}': { get: {} }, '/api/documents/{id}': { get: {} } } },
    empty,
  );
  assert.equal(result.used.length, 2);
});

for (const code of [
  "function load(path: string) { request('GET', path); }",
  "function load(method: string) { request(method, '/api/tasks/42'); }",
  "function load(resource: string) { request('GET', `/api/${resource}/42`); }",
  "function load(suffix: string) { request('GET', `/api/tasks${suffix}`); }",
  "function load(id: string) { request('GET', `/api/tasks/${String(id)}`); }",
  'function wrapper(method: string, path: string) { request(method, path); }',
]) {
  await test(`unresolved call fails: ${code}`, () => {
    assert.throws(() => checkUsage(files(code), contract, empty), /not statically resolvable/);
  });
}

await test('all routes of a conditional must be valid', () => {
  assert.throws(
    () =>
      checkUsage(
        files(`
    function load(ok: boolean) { request('GET', ok ? '/api/tasks/42' : '/api/typo/42'); }
  `),
        contract,
        empty,
      ),
    /GET \/api\/typo\/42 not in OpenAPI/,
  );
});

await test('encoded interpolation cannot stand for a static resource name', () => {
  assert.throws(
    () =>
      checkUsage(
        files(`
    function load(kind: string) { request('GET', \`/api/\${encodeURIComponent(kind)}/42\`); }
  `),
        contract,
        empty,
      ),
    /not in OpenAPI/,
  );
});

await test('locally shadowed encoding helpers are not trusted', () => {
  assert.throws(
    () =>
      checkUsage(
        files(`
    const encodeURIComponent = (v: string) => v;
    function load(id: string) { request('GET', \`/api/tasks/\${encodeURIComponent(id)}\`); }
  `),
        contract,
        empty,
      ),
    /not statically resolvable/,
  );
});

await test('comments, prefix checks, unrelated request functions and URL constants are not calls', () => {
  const input = files(`
    const prefix = '/api'; const publicPrefix = '/api/public';
    const unusedUrl = '/api/tasks/42';
    '/api/tasks/42'.startsWith('/api');
    // request('DELETE', '/api/wrong');
    const guard = { request(action: () => void) {} }; guard.request(() => {});
  `);
  const result = checkUsage(input, contract, {
    forwarders: [],
    unused: [{ method: 'GET', path: '/api/tasks/{task_id}', reason: '가6 판정 예정' }],
  });
  assert.equal(result.calls, 0);
});

await test('renamed imports, namespace imports, reexports and immutable aliases retain identity', () => {
  const input = files('');
  input.set('apps/web/src/lib/bridge.ts', "export { request as send } from '$lib/shared/api';");
  input.set(
    source,
    `
    import { send as renamed } from './bridge';
    import * as api from '$lib/shared/api';
    const alias = renamed;
    alias('GET', '/api/tasks/42'); api.request('GET', '/api/tasks/42');
  `,
  );
  assert.equal(checkUsage(input, contract, empty).calls, 2);
  input.set(source, (input.get(source) ?? '').replace("alias('GET'", "alias('DELETE'"));
  assert.throws(() => checkUsage(input, contract, empty), /DELETE .*not in OpenAPI/);
});

await test('escaping an API helper as a callback fails closed', () => {
  assert.throws(
    () => checkUsage(files('queue(request);'), contract, empty),
    /helper reference is not statically resolvable/,
  );
});

for (const code of [
  'import {request} from "$lib/shared/api"; const wrapped={request}; wrapped.request("DELETE","/api/tasks/42");',
  'import * as api from "$lib/shared/api"; api["request"]("DELETE","/api/tasks/42");',
  'import * as api from "$lib/shared/api"; const {request}=api; request("DELETE","/api/tasks/42");',
  'import * as api from "$lib/shared/api"; function load(name:string){api[name]("DELETE","/api/tasks/42");}',
  'import * as api from "$lib/shared/api"; queue(api);',
]) {
  await test(`indirect API references cannot hide invalid calls: ${code}`, () => {
    const input = files('');
    input.set(source, code);
    assert.throws(
      () =>
        checkUsage(input, contract, {
          forwarders: [],
          unused: [{ method: 'GET', path: '/api/tasks/{task_id}', reason: '가6 판정 예정' }],
        }),
      /not in OpenAPI|not statically resolvable/,
    );
  });
}

const forwarder: Allowlist = {
  forwarders: [
    {
      file: shared,
      name: 'expectOk',
      reason: '가6 판정 예정',
      detail: 'Forward method/path; check callers.',
    },
  ],
  unused: [],
};
function wrapped(code: string): Map<string, string> {
  const input = files('');
  input.set(
    shared,
    `${input.get(shared) ?? ''}
    export function expectOk(method: string, path: string) { return request(method, path); }
  `,
  );
  input.set(source, `import { expectOk } from '$lib/shared/api'; ${code}`);
  return input;
}

await test('reasoned forwarding helpers check their callers and exclude only the forwarding call', () => {
  assert.equal(
    checkUsage(wrapped("expectOk('GET', '/api/tasks/42');"), contract, forwarder).calls,
    1,
  );
  assert.throws(
    () => checkUsage(wrapped("expectOk('DELETE', '/api/tasks/42');"), contract, forwarder),
    /not in OpenAPI/,
  );
  assert.throws(
    () =>
      checkUsage(
        wrapped('function load(path: string) { expectOk("GET", path); }'),
        contract,
        forwarder,
      ),
    /not statically resolvable/,
  );
});

await test('stale or changed forwarding exceptions fail', () => {
  assert.throws(
    () => checkUsage(files("request('GET', '/api/tasks/42');"), contract, forwarder),
    /stale forwarder/,
  );
  const input = wrapped("expectOk('GET', '/api/tasks/42');");
  input.set(
    shared,
    (input.get(shared) ?? '').replace('request(method, path)', 'request(method, path + suffix)'),
  );
  assert.throws(() => checkUsage(input, contract, forwarder), /not statically resolvable/);
});

await test('Svelte script and markup calls are checked with original line numbers', () => {
  const input = files('');
  input.set(
    'apps/web/src/lib/Example.svelte',
    `<script lang="ts">
    import { request as send } from '$lib/shared/api';
    const id = '42';
    send('GET', \`/api/tasks/\${encodeURIComponent(id)}\`);
  </script>
  <button onclick={() => send('GET', '/api/tasks/42')}>load</button>`,
  );
  assert.equal(checkUsage(input, contract, empty).calls, 2);
  input.set(
    'apps/web/src/lib/Example.svelte',
    (input.get('apps/web/src/lib/Example.svelte') ?? '').replace(
      "() => send('GET'",
      "() => send('DELETE'",
    ),
  );
  assert.throws(
    () => checkUsage(input, contract, empty),
    /Example.svelte:6:.*DELETE .*not in OpenAPI/,
  );
});

await test('Svelte condition, loop children and const tags retain nested calls', () => {
  const input = files('');
  input.set(
    'apps/web/src/lib/Nested.svelte',
    `<script lang="ts">
    import { request } from '$lib/shared/api';
    let rows = ['42'];
  </script>
  {#if request('GET', '/api/tasks/42')}
    {#each rows as row}
      {@const value = request('GET', '/api/tasks/42')}
      <button onclick={() => request('GET', '/api/tasks/42')}>{value}</button>
    {/each}
  {/if}`,
  );
  assert.equal(checkUsage(input, contract, empty).calls, 3);
  input.set(
    'apps/web/src/lib/Nested.svelte',
    (input.get('apps/web/src/lib/Nested.svelte') ?? '').replace(
      "() => request('GET'",
      "() => request('DELETE'",
    ),
  );
  assert.throws(() => checkUsage(input, contract, empty), /DELETE .*not in OpenAPI/);
});

await test('unused operations require exact method/path entries; stale and duplicate entries fail', () => {
  const unused: Allowlist = {
    forwarders: [],
    unused: [{ method: 'GET', path: '/api/tasks/{task_id}', reason: '가6 판정 예정' }],
  };
  assert.throws(() => checkUsage(files(''), contract, empty), /unlisted unused operation/);
  assert.deepEqual(checkUsage(files(''), contract, unused).unused, ['GET /api/tasks/{task_id}']);
  assert.throws(
    () => checkUsage(files("request('GET', '/api/tasks/42');"), contract, unused),
    /stale unused entry/,
  );
  assert.throws(() => checkUsage(files(''), { paths: {} }, unused), /stale unused entry/);
  assert.throws(
    () =>
      checkUsage(files(''), contract, { ...unused, unused: [...unused.unused, ...unused.unused] }),
    /duplicate unused entry/,
  );
});

await test('allowlist accepts exactly the two prescribed reason labels and requires forwarding detail', () => {
  assert.throws(
    () =>
      readAllowlist({
        forwarders: [],
        unused: [{ method: 'GET', path: '/healthz', reason: 'temporary' }],
      }),
    /invalid reason/,
  );
  assert.throws(
    () => readAllowlist({ ...forwarder, forwarders: [{ ...forwarder.forwarders[0], detail: '' }] }),
    /nonempty text/,
  );
});

for (const [method, path] of [
  ['GET', '/api/typo/42'],
  ['DELETE', '/api/tasks/42'],
]) {
  await test(`CLI exits FAIL for deliberately invalid ${method ?? ''} ${path ?? ''}`, () => {
    const root = mkdtempSync(join(tmpdir(), 'fvoci-web-api-'));
    try {
      const input = files(`request('${method ?? ''}', '${path ?? ''}');`);
      input.set('contracts/openapi.json', JSON.stringify(contract));
      input.set('xtask/web-api/allowlist.json', JSON.stringify(empty));
      input.set('packages/api-client/src/index.ts', '');
      input.set('packages/api-client/src/generated/schema.ts', '');
      for (const [file, content] of input) {
        const target = join(root, file);
        mkdirSync(dirname(target), { recursive: true });
        writeFileSync(target, content);
      }
      const result = spawnSync(
        process.execPath,
        [fileURLToPath(new URL('./check.mts', import.meta.url)), root],
        { encoding: 'utf8' },
      );
      assert.equal(result.status, 1, result.stderr);
      assert.match(result.stderr, /not in OpenAPI/);
      console.log(`expected FAIL (exit 1): ${method ?? ''} ${path ?? ''}`);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
}
