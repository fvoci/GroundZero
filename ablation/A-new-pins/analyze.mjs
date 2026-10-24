import { readFileSync, writeFileSync } from 'node:fs';
const root = new URL('.', import.meta.url).pathname;
const rows = readFileSync(root + 'measurements.tsv', 'utf8').trim().split('\n').slice(1).map(line => {
  const [pair,order,implementation,mode,name,iterations,nanoseconds,checksum] = line.split('\t');
  return {pair:Number(pair),order,implementation,mode,name,ns:Number(nanoseconds)/Number(iterations), checksum};
});
function median(values) {
  const a = [...values].sort((a,b)=>a-b);
  return (a[(a.length-1)>>1] + a[a.length>>1])/2;
}
const results = [];
for (const mode of ['warm','cold']) for (const name of ['tiny','single_huge','many_ascii','many_unicode','many_blocks']) {
  const subset = rows.filter(r=>r.mode===mode && r.name===name);
  const base = subset.filter(r=>r.implementation==='baseline');
  const variant = subset.filter(r=>r.implementation==='variant');
  const baseMedian = median(base.map(r=>r.ns));
  const variantMedian = median(variant.map(r=>r.ns));
  const ratios = base.map(b=>{
    const v = variant.find(v=>v.pair===b.pair);
    if (v.checksum !== b.checksum) throw new Error('checksum mismatch');
    return 100*(1-v.ns/b.ns);
  });
  const relativeMAD = 100*median(base.map(r=>Math.abs(r.ns-baseMedian)))/baseMedian;
  const pairedMedianImprovement = median(ratios);
  results.push({mode,name,baseMedianNs:baseMedian,variantMedianNs:variantMedian,
    pairedMedianImprovementPct:pairedMedianImprovement,baseRelativeMADPct:relativeMAD,
    pairedRangePct:[Math.min(...ratios),Math.max(...ratios)],
    gate:relativeMAD>5?'NOISY':name.startsWith('many')?(pairedMedianImprovement>Math.max(20,2*relativeMAD)?'PASS':'INSUFFICIENT'):(pairedMedianImprovement>=-5?'PASS':'REGRESSION'),
  });
}
writeFileSync(root+'summary.json',JSON.stringify(results,null,2)+'\n');
process.stdout.write('mode\tcase\tbaseline ns\tvariant ns\tpaired improvement %\tbaseline MAD %\tgate\n');
for (const r of results) process.stdout.write(`${r.mode}\t${r.name}\t${r.baseMedianNs.toFixed(1)}\t${r.variantMedianNs.toFixed(1)}\t${r.pairedMedianImprovementPct.toFixed(2)}\t${r.baseRelativeMADPct.toFixed(2)}\t${r.gate}\n`);
