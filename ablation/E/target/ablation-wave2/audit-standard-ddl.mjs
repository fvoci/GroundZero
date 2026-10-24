import fs from 'node:fs';
import assert from 'node:assert/strict';
const root = 'target/ablation-wave2/';
const before = JSON.parse(fs.readFileSync(root + 'standard-server-before.json', 'utf8'));
const after = JSON.parse(fs.readFileSync(root + 'standard-server-after.json', 'utf8'));
assert.equal(before.system_identifier, after.system_identifier);
assert.equal(before.admin_role_config_is_null, true);
assert.equal(after.admin_role_config_is_null, true);
assert.match(fs.readFileSync(root + 'standard-test-exit.txt', 'utf8'), /test_exit=0\nobserver_restore_exit=0/);
const creates = new Map();
const drops = [];
for (const line of fs.readFileSync(root + 'standard-database-ddl.log', 'utf8').trim().split('\n')) {
  const match = line.match(/\[(\d+)\].*\b(CREATE|DROP) DATABASE ([a-zA-Z0-9_]+)/);
  assert.ok(match, line);
  const [, pid, operation, name] = match;
  if (operation === 'CREATE') {
    assert.ok(!creates.has(name));
    assert.ok(!before.databases.includes(name));
    creates.set(name, { name, pid: Number(pid), create_log: line });
  } else {
    assert.ok(creates.has(name), 'DROP without earlier CREATE');
    assert.equal(creates.get(name).pid, Number(pid), 'CREATE/DROP session mismatch');
    assert.ok(!drops.some(drop => drop.name === name));
    drops.push({ ...creates.get(name), drop_log: line });
  }
}
const retained = [...creates.keys()].filter(name => !drops.some(drop => drop.name === name)).sort();
assert.deepEqual(after.databases.filter(name => !before.databases.includes(name)).sort(), retained);
for (const name of before.databases) assert.ok(after.databases.includes(name));
const benchmark = JSON.parse(fs.readFileSync(root + 'benchmark-after-standard.json', 'utf8'));
const expected = JSON.parse(fs.readFileSync(root + 'current-cardinality.json', 'utf8')).global_counts;
for (const [key, value] of Object.entries(expected)) assert.equal(benchmark[key], value);
const result = {
  task_id: 'task_be5df1e065f8', dispatch_id: 'ctx_628d1bc7eac9',
  source_commit: fs.readFileSync(root + 'standard-test-head.txt', 'utf8').trim(),
  server_system_identifier: before.system_identifier,
  project: 'fvoci-ablation-home-marks-f0352d-test-standard',
  start: fs.readFileSync(root + 'standard-test-start.txt', 'utf8').trim(),
  end: fs.readFileSync(root + 'standard-test-end.txt', 'utf8').trim(),
  evidence: 'DDL is logged before execution; successful CREATE is supported by .await.unwrap() before FreshDb return and the passing full suite, then same-name/same-session DROP lineage.',
  source_lineage: ['tests/common/mod.rs:fresh_database', 'tests/migration_baseline.rs:empty_database', 'tests/backup.rs:target/source.name cleanup', 'tests/search.rs:fresh.name cleanup'],
  created_count: creates.size, dropped_count: drops.length, dropped: drops, retained,
  protected_initial_databases_preserved: true, admin_role_defaults_restored: true,
  original_benchmark_counts_unchanged: true,
};
fs.writeFileSync(root + 'standard-ddl-audit.json', JSON.stringify(result, null, 2) + '\n');
process.stdout.write(JSON.stringify({created:creates.size,dropped:drops.length,retained:retained.length,verified:true})+'\n');
