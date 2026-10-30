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

const listedTask: Allowlist = {
  forwarders: [],
  unused: [{ method: 'GET', path: '/api/tasks/{task_id}', reason: '샴 가3~가5 조립 예정' }],
};
for (const [method, path] of [
  ['GET', '/api/typo/42'],
  ['DELETE', '/api/tasks/42'],
]) {
  await test(`SR-250-3: default fetch parameter checks ${method ?? ''} ${path ?? ''}`, () => {
    assert.throws(
      () =>
        checkUsage(
          files(`
      function load(fetchImpl: typeof fetch = fetch) {
        return fetchImpl('${path ?? ''}', { method: '${method ?? ''}' });
      }
    `),
          contract,
          listedTask,
        ),
      /not in OpenAPI/,
    );
  });
}
await test('SR-250-3: unsupported fetch references fail', () => {
  assert.throws(
    () => checkUsage(files('queue(fetch);'), contract, listedTask),
    /fetch reference.*not statically resolvable/,
  );
});
for (const code of [
  "window.location.assign('/api/typo/42');",
  "window.open('/api/typo/42');",
  "const a = document.createElement('a'); a.href = '/api/typo/42';",
  "const a = document.createElement('a'); a.setAttribute('href', '/api/typo/42');",
  "window.location.href = '/api/typo/42';",
  "location = '/api/typo/42';",
]) {
  await test(`SR-250-4: navigation checks paths: ${code}`, () => {
    assert.throws(
      () => checkUsage(files(code), contract, listedTask),
      /GET \/api\/typo\/42.*not in OpenAPI/,
    );
  });
}
await test('SR-250-4: links use GET and detect a method mismatch', () => {
  const input = files('');
  input.set('apps/web/src/lib/Link.svelte', '<a href="/api/tasks/42">link</a>');
  assert.throws(
    () =>
      checkUsage(
        input,
        { paths: { '/api/tasks/{id}': { post: {} } } },
        {
          forwarders: [],
          unused: [{ method: 'POST', path: '/api/tasks/{id}', reason: '샴 가3~가5 조립 예정' }],
        },
      ),
    /GET \/api\/tasks\/42.*not in OpenAPI/,
  );
});
for (const [path, expected] of [
  ['/api/typo', /GET \/api\/typo.*not in OpenAPI/],
  ['/rhwp/../api/typo', /dot segment/],
] as const) {
  await test(`SR-250-12: navigation follows the caller argument ${path}`, () => {
    assert.throws(
      () =>
        checkUsage(
          files(`function go(p: string) { location.assign(p); } go('${path}');`),
          contract,
          listedTask,
        ),
      expected,
    );
  });
}
await test('SR-250-12: runtime callers do not hide a known invalid link argument', () => {
  assert.throws(
    () =>
      checkUsage(
        files(`
          const link = (p: string) => { anchor.href = p; };
          function go(p: string) { link(p); }
          go(userContent); go('/api/typo');
        `),
        contract,
        listedTask,
      ),
    /GET \/api\/typo.*not in OpenAPI/,
  );
});
await test('SR-250-12: forwarded links still use GET', () => {
  assert.throws(
    () =>
      checkUsage(
        files(`const go = function (p: string) { window.open(p); }; go('/api/tasks/42');`),
        { paths: { '/api/tasks/{id}': { post: {} } } },
        {
          forwarders: [],
          unused: [{ method: 'POST', path: '/api/tasks/{id}', reason: '샴 가3~가5 조립 예정' }],
        },
      ),
    /GET \/api\/tasks\/42.*not in OpenAPI/,
  );
});
await test('SR-250-12: defaults and nested arguments resolve used operations', () => {
  const result = checkUsage(
    files(`
      const link = (p: string) => { anchor.setAttribute('href', p); };
      function go(p = '/api/tasks/42') { link(p); }
      go(); go(userContent); go('/rhwp/viewer');
    `),
    contract,
    empty,
  );
  assert.equal(result.calls, 1);
  assert.deepEqual(result.used, ['GET /api/tasks/{task_id}']);
  assert.deepEqual(result.unused, []);
});
await test('SR-250-12: runtime-only callers remain outside the check', () => {
  assert.equal(
    checkUsage(
      files(`function go(p: string) { location.assign(p); } go(userContent);`),
      contract,
      listedTask,
    ).calls,
    0,
  );
});
await test('SR-250-12: recursive callers cannot hide a literal inside the function', () => {
  assert.throws(
    () =>
      checkUsage(
        files(`function go() { location.assign('/api/typo'); go(); }`),
        contract,
        listedTask,
      ),
    /GET \/api\/typo.*not in OpenAPI/,
  );
});
for (const path of ['/rhwp/%2e%2e/api/typo', '/app/../page', 'https://example.org/../page']) {
  await test(`SR-250-12: forwarded literals reject dot segments before prefix classification: ${path}`, () => {
    assert.throws(
      () =>
        checkUsage(
          files(`function go(p: string) { location.assign(p); } go('${path}');`),
          contract,
          listedTask,
        ),
      /dot segment/,
    );
  });
}
await test('SR-250-4: undeclared WebSocket routes fail', () => {
  assert.throws(
    () => checkUsage(files("new WebSocket('wss://example.org/api/wrong');"), contract, listedTask),
    /WebSocket.*exception|unlisted WebSocket/,
  );
});
for (const [method, path] of [
  ['GET', '/rhwp/../api/typo/42'],
  ['DELETE', '/rhwp/%2e%2e/api/tasks/42'],
  ['GET', '/rhwp/.%2e/api/typo/42'],
]) {
  await test(`SR-250-5: reject dot segments before classifying ${method ?? ''} ${path ?? ''}`, () => {
    assert.throws(
      () =>
        checkUsage(
          files(`fetch('${path ?? ''}', { method: '${method ?? ''}' });`),
          contract,
          listedTask,
        ),
      /dot segment/,
    );
  });
}
await test('SR-250-1: direct fetch with a wrong path fails', () => {
  assert.throws(
    () => checkUsage(files("fetch('/api/typo/42');"), contract, listedTask),
    /GET \/api\/typo\/42.*not in OpenAPI/,
  );
});
await test('SR-250-1: direct fetch with a wrong method fails', () => {
  assert.throws(
    () => checkUsage(files("fetch('/api/tasks/42', { method: 'DELETE' });"), contract, listedTask),
    /DELETE \/api\/tasks\/42.*not in OpenAPI/,
  );
});
for (const [code, expected] of [
  ['fetch(`//${location.host}/api/typo/42`);', /GET \/api\/typo\/42.*not in OpenAPI/],
  [
    'fetch(`//${window.location.host}/api/tasks/42`, { method: "DELETE" });',
    /DELETE \/api\/tasks\/42.*not in OpenAPI/,
  ],
] as const) {
  await test(`CodeRabbit: same-origin protocol-relative fetch is checked: ${code}`, () => {
    assert.throws(() => checkUsage(files(code), contract, listedTask), expected);
  });
}
await test('same-origin protocol-relative fetch marks the contract operation used', () => {
  const result = checkUsage(files('fetch(`//${location.host}/api/tasks/42`);'), contract, empty);
  assert.equal(result.calls, 1);
  assert.deepEqual(result.used, ['GET /api/tasks/{task_id}']);
  assert.deepEqual(result.unused, []);
});
await test('external protocol-relative fetch remains outside the contract check', () => {
  const result = checkUsage(
    files(`
      fetch('//example.org/api/typo/42');
      fetch('//fvoci.invalid.example.org/api/typo/42');
    `),
    contract,
    listedTask,
  );
  assert.equal(result.calls, 0);
  assert.deepEqual(result.unused, ['GET /api/tasks/{task_id}']);
});

await test('direct fetch uses GET by default, honors method options and excludes used operations from unused', () => {
  const input = files(`
    const method = 'GET';
    const base = { method: 'DELETE' };
    const options = { ...base, method, ...{ credentials: 'same-origin' } };
    fetch('/api/tasks/42');
    fetch('/api/tasks/42', options);
    fetch('/api/tasks/42', { method: undefined });
    window.fetch('/api/tasks/42');
    const send = fetch; send('/api/tasks/42');
  `);
  const result = checkUsage(input, contract, empty);
  assert.equal(result.calls, 5);
  assert.deepEqual(result.unused, []);
  assert.throws(() => checkUsage(input, contract, listedTask), /stale unused entry/);
  assert.throws(
    () =>
      checkUsage(
        files("fetch('/api/tasks/42', { method: 'GET', ...{ method: 'DELETE' } });"),
        contract,
        listedTask,
      ),
    /DELETE .*not in OpenAPI/,
  );
});

await test('direct fetch checks upload templates, healthz and auth routes', () => {
  const result = checkUsage(
    files(`
    function put(upload: string, number: number) {
      fetch(\`/api/uploads/\${encodeURIComponent(upload)}/parts/\${String(number)}\`, { method: 'PUT' });
    }
    fetch('/healthz', { cache: 'no-store' });
    fetch('/auth/logout', { method: 'POST' });
  `),
    {
      paths: {
        '/api/uploads/{upload_id}/parts/{number}': { put: {} },
        '/healthz': { get: {} },
        '/auth/logout': { post: {} },
      },
    },
    empty,
  );
  assert.equal(result.calls, 3);
  assert.throws(
    () => checkUsage(files("fetch('/auth/missing');"), { paths: {} }, empty),
    /not in OpenAPI/,
  );
});

await test('direct fetch resolves imported path functions and function arguments in Svelte', () => {
  const input = files('');
  input.set(
    'apps/web/src/lib/urls.ts',
    `
    export function fileContentUrl(id: string, version?: number) {
      const query = version === undefined ? '' : \`?version=\${String(version)}\`;
      return \`/api/files/\${encodeURIComponent(id)}/content\${query}\`;
    }
    export const resourceUrl = (resource: string, id: string) => \`/api/\${resource}/\${encodeURIComponent(id)}\`;
  `,
  );
  input.set(
    source,
    `
    import { fileContentUrl, resourceUrl } from './urls';
    function download(id: string) { fetch(fileContentUrl(id, 2), { credentials: 'same-origin' }); }
    function load(id: string) { fetch(resourceUrl('tasks', id)); }
  `,
  );
  input.set(
    'apps/web/src/lib/Preview.svelte',
    `<script lang="ts">
    import { fileContentUrl } from './urls'; let id = 'file';
  </script><button onclick={() => fetch(fileContentUrl(id))}>preview</button>`,
  );
  const routes = {
    paths: { '/api/files/{id}/content': { get: {} }, '/api/tasks/{id}': { get: {} } },
  };
  assert.equal(checkUsage(input, routes, empty).calls, 3);
  input.set(
    'apps/web/src/lib/urls.ts',
    (input.get('apps/web/src/lib/urls.ts') ?? '').replace('/content', '/typo'),
  );
  assert.throws(
    () => checkUsage(input, routes, empty),
    /GET \/api\/files\/\{\}\/typo not in OpenAPI/,
  );
});

await test('CodeRabbit: reassigned let URL helpers must fail instead of following their initializer', () => {
  assert.throws(
    () =>
      checkUsage(
        files(`
    let url = () => '/api/tasks/42';
    url = () => '/api/typo/42';
    fetch(url());
  `),
        contract,
        empty,
      ),
    /URL helper.*const|not statically resolvable/,
  );
  assert.throws(
    () =>
      checkUsage(
        files(`
    let url = () => '42'; url = () => '..';
    fetch('/api/tasks/' + encodeURIComponent(url()));
  `),
        contract,
        empty,
      ),
    /URL helper.*const/,
  );
});

await test('direct fetch ignores resources outside contract namespaces and unrelated fetch functions', () => {
  const result = checkUsage(
    files(`
    const scriptUrl = '/pptx-frame.js'; fetch(scriptUrl);
    fetch('/rhwp/manifest.json'); fetch('https://example.org/api/tasks/42');
    const local = { fetch(path: string) {} }; local.fetch('/api/typo');
  `),
    { paths: {} },
    empty,
  );
  assert.equal(result.calls, 0);
});

for (const code of [
  'function load(url: string) { fetch(url); }',
  'function load(name: string) { fetch(`/rhwp/${name}`); }',
  'function load(host: string) { fetch(`https://${host}/api/data`); }',
  "function load(method: string) { fetch('/api/tasks/42', { method }); }",
  "function load(options: RequestInit) { fetch('/api/tasks/42', options); }",
  "fetch('api/tasks/42');",
  "const options = { method: 'GET' }; options.method = 'DELETE'; fetch('/api/tasks/42', options);",
  "const options = { method: 'GET' }; options['method'] = 'DELETE'; fetch('/api/tasks/42', options);",
]) {
  await test(`unresolved or changed direct fetch fails: ${code}`, () => {
    assert.throws(
      () => checkUsage(files(code), contract, listedTask),
      /not statically resolvable|options.method is written/,
    );
  });
}

await test('shared request fetch transport is counted through its callers and must forward the method', () => {
  const input = files("request('GET', '/api/tasks/42');");
  input.set(
    shared,
    `export function request(method: string, path: string) {
    const init = { method, credentials: 'same-origin' }; return fetch(path, init);
  }`,
  );
  assert.equal(checkUsage(input, contract, empty).calls, 1);
  input.set(shared, (input.get(shared) ?? '').replace('{ method,', "{ method: 'DELETE',"));
  assert.throws(() => checkUsage(input, contract, empty), /transport method is not forwarded/);
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
    unused: [{ method: 'GET', path: '/api/tasks/{task_id}', reason: '샴 가3~가5 조립 예정' }],
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
          unused: [{ method: 'GET', path: '/api/tasks/{task_id}', reason: '샴 가3~가5 조립 예정' }],
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
      reason: '설계상 직접 호출하지 않음',
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

for (const write of [
  "path = '/api/typo/42';",
  "method = 'DELETE';",
  "path += '/typo';",
  'path++;',
  '--method;',
  "[path] = ['/api/typo/42'];",
  "({ path } = { path: '/api/typo/42' });",
  "({ other: method } = { other: 'DELETE' });",
  "const change = () => { path = '/api/typo/42'; }; change();",
  "({ path = '/api/typo/42' } = {});",
  "for (path of ['/api/typo/42']) {}",
  "method ||= 'DELETE';",
]) {
  await test(`SR-250-2: forwarder parameter writes fail: ${write}`, () => {
    const input = wrapped("expectOk('GET', '/api/tasks/42');");
    input.set(
      shared,
      (input.get(shared) ?? '').replace(
        'return request(method, path)',
        `${write} return request(method, path)`,
      ),
    );
    assert.throws(() => checkUsage(input, contract, forwarder), /parameter.*written/);
  });
}

await test('forwarder parameter writes after delegation fail; shadowed variables and object fields do not', () => {
  const input = wrapped("expectOk('GET', '/api/tasks/42');");
  input.set(
    shared,
    (input.get(shared) ?? '').replace(
      'return request(method, path)',
      `
    function inner(path: string) { path = '/api/typo'; }
    const data = { path }; data.path = '/api/typo';
    return request(method, path)
  `,
    ),
  );
  assert.equal(checkUsage(input, contract, forwarder).calls, 1);
  input.set(
    shared,
    (input.get(shared) ?? '').replace(
      'return request(method, path)',
      "request(method, path); path = '/api/typo';",
    ),
  );
  assert.throws(() => checkUsage(input, contract, forwarder), /parameter path is written/);
});

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
    unused: [
      {
        method: 'GET',
        path: '/api/tasks/{task_id}',
        reason: '설계상 직접 호출하지 않음',
        detail: 'This fixture has no direct caller; only the exact listed operation is exempt.',
      },
    ],
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

await test('allowlist rejects unknown reasons and requires forwarding detail', () => {
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

await test('design exceptions preserve the required detail for unused operations and forwarders', () => {
  const reason = '설계상 직접 호출하지 않음';
  const detail = 'OIDC provider redirects the browser to this callback after login.';
  const input = {
    forwarders: [{ ...forwarder.forwarders[0], reason }],
    unused: [{ method: 'GET', path: '/auth/callback', reason, detail }],
  };
  assert.deepEqual(readAllowlist(input), input);
});

for (const detail of [undefined, null, '', ' \t ', 42]) {
  await test(`design exceptions require nonempty text detail: ${String(detail)}`, () => {
    const reason = '설계상 직접 호출하지 않음';
    assert.throws(
      () =>
        readAllowlist({
          forwarders: [],
          unused: [{ method: 'GET', path: '/auth/callback', reason, detail }],
        }),
      /nonempty text/,
    );
    assert.throws(
      () =>
        readAllowlist({ ...empty, forwarders: [{ ...forwarder.forwarders[0], reason, detail }] }),
      /nonempty text/,
    );
  });
}

await test('retired pending judgment reason is rejected for unused operations and forwarders', () => {
  const reason = '가6 판정 예정';
  assert.throws(
    () =>
      readAllowlist({
        ...empty,
        unused: [{ method: 'GET', path: '/auth/callback', reason, detail: 'Still pending.' }],
      }),
    /invalid reason/,
  );
  assert.throws(
    () => readAllowlist({ ...empty, forwarders: [{ ...forwarder.forwarders[0], reason }] }),
    /invalid reason/,
  );
});

await test('fetch aliases include defaults, const aliases, global members and destructuring', () => {
  const input = files(`
    function load(send: typeof fetch = globalThis['fetch']) { const invoke = send; invoke('/api/tasks/42'); }
    const { fetch: invoke } = window; invoke('/api/tasks/42');
    load(fetch);
  `);
  assert.equal(checkUsage(input, contract, empty).calls, 2);
});

for (const declaration of [
  "const load = (send: typeof fetch = fetch) => send('/api/tasks/42');",
  "const load = function (send: typeof fetch = fetch) { return send('/api/tasks/42'); };",
]) {
  await test(`CodeRabbit: const function fetch defaults reject an unknown override: ${declaration}`, () => {
    assert.throws(
      () => checkUsage(files(`${declaration} load(other);`), contract, empty),
      /override|not statically resolvable/,
    );
  });
}
for (const code of [
  'let invoke = fetch; invoke("/api/tasks/42");',
  'const invoke = fetch.bind(window); invoke("/api/tasks/42");',
  'function load(send: typeof fetch) { send("/api/tasks/42"); }',
  'function load(send: typeof fetch = fetch) { send = other; send("/api/tasks/42"); }',
  'function load(send: typeof fetch = fetch) { send("/api/tasks/42"); } load(other);',
]) {
  await test(`unresolved fetch capability fails: ${code}`, () => {
    assert.throws(
      () => checkUsage(files(code), contract, listedTask),
      /fetch reference|fetch parameter/,
    );
  });
}

await test('Svelte and HTML links are checked as GET', () => {
  const input = files('');
  input.set('apps/web/src/lib/Links.svelte', '<a href="/api/tasks/42">ok</a>');
  input.set('apps/web/index.html', '<html><body><a href="/api/tasks/42">ok</a></body></html>');
  assert.equal(checkUsage(input, contract, empty).calls, 2);
});

await test('named WebSocket item checks room composition and never exempts HTTP', () => {
  const input = files(`
    import { WebsocketProvider as Provider } from 'y-websocket';
    new Provider('wss://example.org/api/tasks/42', 'collab', doc);
  `);
  const named: Allowlist = {
    ...empty,
    websocket: {
      name: 'document_collaboration',
      path: '/api/tasks/{id}/collab',
      reason: 'CRDT upgrade outside OpenAPI',
    },
  };
  assert.deepEqual(checkUsage(input, { paths: {} }, named).unused, []);
  input.set(source, (input.get(source) ?? '').replace("'collab'", "'typo'"));
  assert.throws(() => checkUsage(input, { paths: {} }, named), /unlisted WebSocket/);
  assert.throws(
    () =>
      checkUsage(files("fetch('/api/tasks/42/collab', {method: 'DELETE'});"), { paths: {} }, named),
    /DELETE .*not in OpenAPI/,
  );
  assert.throws(
    () => readAllowlist({ ...named, websocket: { ...named.websocket, reason: '' } }),
    /nonempty text/,
  );
});
await test('app paths, static resources and external URLs need no per-site exceptions', () => {
  const input = files(`
    fetch('/rhwp/manifest.json', {method: unknownMethod});
    fetch('https://example.org/api/typo');
    function move(url: string) { window.location.replace(url); }
  `);
  input.set('apps/web/src/lib/Link.svelte', '<script>let route;</script><a href={route}>route</a>');
  assert.equal(checkUsage(input, { paths: {} }, empty).calls, 0);
});

for (const code of [
  "const suffix = '../api/typo'; fetch('/rhwp/' + suffix);",
  "fetch(`/rhwp/${encodeURIComponent('..')}/api/typo`);",
  "window.location.assign('/rhwp/.%2e/api/typo');",
]) {
  await test(`noncanonical paths fail across outlets: ${code}`, () => {
    assert.throws(() => checkUsage(files(code), contract, listedTask), /dot segment/);
  });
}

for (const [kind, method, path] of [
  ['request', 'GET', '/api/typo/42'],
  ['request', 'DELETE', '/api/tasks/42'],
  ['fetch', 'GET', '/api/typo/42'],
  ['fetch', 'DELETE', '/api/tasks/42'],
  ['fetch parameter', 'GET', '/api/typo/42'],
  ['fetch parameter', 'DELETE', '/api/tasks/42'],
  ['location', 'GET', '/api/typo/42'],
  ['location parameter', 'GET', '/api/typo'],
  ['location parameter', 'GET', '/rhwp/../api/typo'],
  ['WebSocket', 'GET', '/api/typo/42'],
  ['fetch', 'GET', '/rhwp/../api/typo/42'],
  ['fetch', 'DELETE', '/rhwp/%2e%2e/api/tasks/42'],
]) {
  await test(`CLI exits FAIL for deliberately invalid ${kind ?? ''} ${method ?? ''} ${path ?? ''}`, () => {
    const root = mkdtempSync(join(tmpdir(), 'fvoci-web-api-'));
    try {
      const input = files(
        kind === 'fetch'
          ? `fetch('${path ?? ''}', { method: '${method ?? ''}' });`
          : kind === 'fetch parameter'
            ? `function load(fetchImpl: typeof fetch = fetch) { fetchImpl('${path ?? ''}', {method: '${method ?? ''}'}); }`
            : kind === 'location'
              ? `window.location.assign('${path ?? ''}');`
              : kind === 'location parameter'
                ? `function go(p: string) { location.assign(p); } go('${path ?? ''}');`
                : kind === 'WebSocket'
                  ? `new WebSocket('wss://example.org${path ?? ''}');`
                  : `request('${method ?? ''}', '${path ?? ''}');`,
      );
      input.set('contracts/openapi.json', JSON.stringify(contract));
      input.set(
        'xtask/web-api/allowlist.json',
        JSON.stringify(kind === 'request' ? empty : listedTask),
      );
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
      assert.match(
        result.stderr,
        kind === 'WebSocket'
          ? /unlisted WebSocket/
          : path?.includes('..') || path?.includes('%2e')
            ? /dot segment/
            : /not in OpenAPI/,
      );
      console.log(`expected FAIL (exit 1): ${kind ?? ''} ${method ?? ''} ${path ?? ''}`);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
}
