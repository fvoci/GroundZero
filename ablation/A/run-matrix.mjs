import { execFileSync } from 'node:child_process';
import { appendFileSync, writeFileSync } from 'node:fs';
import { loadavg, freemem } from 'node:os';

const root = new URL('.', import.meta.url).pathname;
const rows = root + 'measurements.tsv';
writeFileSync(rows, 'pair\torder\timplementation\tmode\tcase\titerations\tnanoseconds\tchecksum\n');
writeFileSync(root + 'measurement-environment.json', JSON.stringify({
  started: new Date().toISOString(), loadavg: loadavg(), freemem: freemem(),
  node: process.version, repetitions: 10,
  modes: 'warm uses five untimed calls; process-cold is a fresh process with no explicit warmup, reporting the whole batch',
}, null, 2));
for (const mode of ['warm', 'cold']) {
  for (const name of ['tiny', 'single_huge', 'many_ascii', 'many_unicode', 'many_blocks']) {
    for (let pair = 0; pair < 10; pair++) {
      const competing = execFileSync('ps', ['-eo','comm=,stat='], { encoding: 'utf8' }).split('\n')
        .filter(line => /^(rustc|cargo|pnpm|cc1|cc1plus|ld|sccache|xtask|vitest|fvoci-server|(?:search|export|cache|collab)-\S*)\s/.test(line.trim()));
      appendFileSync(root + 'process-monitor.jsonl', JSON.stringify({ at:new Date().toISOString(), mode, name, pair, competing })+'\n');
      const order = pair % 2 === 0 ? ['baseline', 'variant'] : ['variant', 'baseline'];
      for (const implementation of order) {
        const value = execFileSync(root + implementation, ['measure', mode, name], { encoding: 'utf8' }).trim();
        appendFileSync(rows, `${pair}\t${order.join(',')}\t${implementation}\t${value}\n`);
      }
    }
    process.stdout.write(`finished ${mode} ${name}\n`);
  }
}
appendFileSync(root + 'measurement-environment.jsonl', JSON.stringify({ finished: new Date().toISOString(), loadavg: loadavg(), freemem: freemem() }) + '\n');
