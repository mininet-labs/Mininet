import {readFileSync,writeFileSync} from 'node:fs';
const read=p=>readFileSync(new URL(p,import.meta.url),'utf8');
const bundle=['population.mjs','compensation.mjs','generations.mjs'].map(p=>read(p).replace(/^import .*;\n/gm,'').replace(/^export /gm,'')).join('\n');
if(bundle.includes('</script'))throw new Error('Unsafe script terminator in model source.');
writeFileSync(new URL('./index.html',import.meta.url),read('review.html').replace('/* MODEL_BUNDLE */',()=>bundle));
console.log('Built standalone offline review tool.');
