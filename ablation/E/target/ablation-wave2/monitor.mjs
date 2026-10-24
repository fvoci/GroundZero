import fs from 'node:fs';
const out=process.argv[2];
let old=new Map();
function sample() {
 const time=new Date().toISOString();
 const rows=[];const next=new Map();
 for(const pid of fs.readdirSync('/proc').filter(s=>/^\d+$/.test(s))) {
  try {
   const s=fs.readFileSync(`/proc/${pid}/stat`,'utf8');
   const close=s.lastIndexOf(')');const name=s.slice(s.indexOf('(')+1,close);
   const fields=s.slice(close+2).split(' ');const cpu=Number(fields[11])+Number(fields[12]);
   const key=pid+':'+fields[19];const previous=old.get(key);
   next.set(key,cpu);
   if(previous!==undefined&&cpu>previous) rows.push({pid:Number(pid),name,ticks:cpu-previous});
   else if(previous===undefined&&old.size>0) rows.push({pid:Number(pid),name,new_process:true});
  } catch { /* process exited between enumeration and read */ }
 }
 fs.appendFileSync(out,JSON.stringify({time,load:fs.readFileSync('/proc/loadavg','utf8').trim(),cpu:fs.readFileSync('/proc/stat','utf8').split('\n')[0],active:rows})+'\n');
 old=next;
}
sample();setInterval(sample,1000);
