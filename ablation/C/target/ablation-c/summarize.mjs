import fs from 'node:fs';
const quantile = (a,p) => [...a].sort((a,b)=>a-b)[Math.floor((a.length-1)*p)];
const stats = a => ({p50:quantile(a,.5),p95:quantile(a,.95),MAD:quantile(a.map(v=>Math.abs(v-quantile(a,.5))),.5)});
for (const file of process.argv.slice(2)) {
  console.log(file);
  const rows = fs.readFileSync(file,'utf8').trim().split('\n').slice(1).map(line=>line.split(','));
  for(const name of ['hex','empty','small','large']) {
    const r=rows.filter(r=>r[0]===name && +r[1]>=0);
    const a=r.map(r=>+r[4]),b=r.map(r=>+r[5]),ratios=r.map(r=>+r[5]/+r[4]);
    console.log(JSON.stringify({case:name,n:r.length,baseline_ns:[.1,.5,.9].map(p=>quantile(a,p)),candidate_ns:[.1,.5,.9].map(p=>quantile(b,p)),paired_ratio_p10_p50_p90:[.1,.5,.9].map(p=>quantile(ratios,p)),baseline:stats(a),candidate:stats(b),ratio:stats(ratios),faster_pairs:ratios.filter(r=>r<1).length,cold:rows.find(r=>r[0]===name&&+r[1]===-1)?.slice(4)}));
  }
}
