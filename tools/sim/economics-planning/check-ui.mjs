// DOM-contract smoke check, not a browser rendering or accessibility audit.
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const html=readFileSync(new URL('./index.html',import.meta.url),'utf8');
const nodes=new Map();
for(const m of html.matchAll(/<([a-z]+)[^>]*\bid="([^"]+)"[^>]*>/g)){
  let value=m[0].match(/\bvalue="([^"]*)"/)?.[1]??'';
  if(m[1]==='select')value=html.slice(m.index).match(/<option value="([^"]+)"/)?.[1]??'';
  nodes.set(m[2],{value,innerHTML:'',textContent:'',clientWidth:550,addEventListener(){}});
}
const context=vm.createContext({document:{getElementById:id=>{assert(nodes.has(id),'Missing '+id);return nodes.get(id);},querySelectorAll:()=>[...nodes.values()]},window:{addEventListener(){}},setTimeout,clearTimeout,Intl,URL,Blob,console});
const script=html.match(/<script>([\s\S]*)<\/script>/)[1];
vm.runInContext(script,context,{timeout:15000});
assert.equal(nodes.get('error').textContent,'');assert(nodes.get('checkpoints').innerHTML.includes('3026'));
assert(nodes.get('pay').innerHTML.includes('177,000'));
nodes.get('capturePct').value='1';vm.runInContext('render()',context,{timeout:15000});
assert(Math.abs(vm.runInContext('latest.rows[0].priceTotal',context)-5000)<1e-9);
nodes.get('supply').value='bad';vm.runInContext('render()',context,{timeout:15000});
assert(nodes.get('error').textContent.includes('Opening supply'));assert.equal(nodes.get('checkpoints').textContent,'');
assert(!/<script[^>]+src=|<link[^>]+href=|\bfetch\(/.test(html));
console.log('Passed offline DOM-contract smoke check: first render, changed assumption, invalid input, no runtime network dependency. Browser visual QA not performed.');
