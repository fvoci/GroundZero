import fs from 'node:fs';
const source=process.argv[2] ?? 'target/ablation-wave2/latency.csv';
const lines=fs.readFileSync(source,'utf8').trim().split('\n');
const keys=lines.shift().split(',');
const rows=lines.map(l=>Object.fromEntries(l.split(',').map((v,i)=>[keys[i], ['phase','variant'].includes(keys[i])?v:Number(v)])));
const quantile=(xs,p)=>{const a=xs.toSorted((x,y)=>x-y);return a[Math.ceil(p*a.length)-1];};
const median=xs=>{const a=xs.toSorted((x,y)=>x-y);return (a[Math.floor((a.length-1)/2)]+a[Math.ceil((a.length-1)/2)])/2;};
const mad=xs=>{const m=median(xs);return median(xs.map(x=>Math.abs(x-m)));};
const summaries=[];
for(const n of [0,1,100]) for(const phase of ['plan_first','warm']) {
 const sample=rows.filter(r=>r.n===n&&r.phase===phase);
 const variants={};
 for(const v of ['A','B']) {const xs=sample.filter(r=>r.variant===v).map(r=>r.microseconds);variants[v]={samples:xs.length,p50_us:median(xs),p95_us:quantile(xs,.95),MAD_us:mad(xs)};}
 const deltas=[];
 for(const block of [...new Set(sample.map(r=>r.block))]) {
  const a=sample.filter(r=>r.block===block&&r.variant==='A').map(r=>r.microseconds);
  const b=sample.filter(r=>r.block===block&&r.variant==='B').map(r=>r.microseconds);
  deltas.push(100*(median(b)/median(a)-1));
 }
 const paired=median(deltas),noise=mad(deltas);
 summaries.push({n,phase,...variants,paired_delta_percent:paired,paired_MAD_pp:noise,regression_gate:paired>5&&paired>3*noise?'FAIL':'PASS_NO_REPEATABLE_REGRESSION',gain_above_noise:paired<0&&Math.abs(paired)>3*noise});
}
console.log(JSON.stringify({scope:'optimized release typed marks fetch/filter/split stage only; synthetic personal documents, same targets in both categories',source,summaries},null,2));
