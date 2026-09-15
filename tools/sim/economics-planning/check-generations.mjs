import assert from 'node:assert/strict';
import {forecast,longDefaults,validateLong,U128_MAX} from './generations.mjs';
import {populationData} from './population.mjs';
import {writeFileSync} from 'node:fs';
assert.equal(populationData.rows.length,75);
assert.equal(populationData.rows[0].population,8300678395);
assert.equal(populationData.rows.at(-1).population,10180160751);
const summaries=[];
for(const populationPath of ['current','stable','decline','expansion'])for(const annualGuard of [true,false]){
  const result=forecast({populationPath,annualGuard});assert(result.conservation);
  for(const r of result.rows){
    assert.equal(BigInt(r.totalMicro),BigInt(r.circulatingMicro)+BigInt(r.lockedMicro));
    assert(BigInt(r.totalMicro)<=U128_MAX);
    if(r.elapsed&&annualGuard)assert(BigInt(r.humanMicro)+BigInt(r.serviceMicro)+BigInt(r.treasuryMicro)<=BigInt(r.openingMicro)*3n/100n);
    assert(Math.abs(r.priceTotal*r.totalMINI-r.attributedWealth)<=Math.max(1,r.attributedWealth*1e-12));
    assert(r.humanVestedMINI>=0);
  }
  summaries.push({populationPath,annualGuard,maxAnnual:result.maxAnnual,rows:[0,1,74,100,200,1000].map(i=>result.rows[i])});
}
const a=forecast({years:100}), b=forecast({years:100,capturePct:1});
assert.equal(a.rows[100].totalMicro,b.rows[100].totalMicro);
assert(Math.abs(a.rows[100].priceTotal/100-b.rows[100].priceTotal)<1e-9);
const partial=forecast({years:1,activePct:50});assert(partial.rows[1].humanAccruedPerEligible>a.rows[1].humanAccruedPerEligible);
const sybil=forecast({years:1,sybilPct:100});assert(sybil.rows[1].humanAccruedPerEligible<a.rows[1].humanAccruedPerEligible);
assert(sybil.rows[1].sybilHumanUSD>0);
const zero=forecast({years:1,capturePct:0,servicePct:0,treasuryPct:0});assert.equal(zero.rows[1].priceTotal,0);assert.equal(zero.rows[1].serviceMicro,'0');assert.equal(zero.rows[1].treasuryMicro,'0');
// Micro-MINI rounding cannot create an unfunded equal allocation; unminted dust remains unissued.
const tiny=forecast({years:1,supply:'1'});assert.equal(tiny.rows[1].humanMicro,'0');assert(BigInt(tiny.rows[1].unmintedHumanRemainderMicro)>0n);
assert(a.rows[1].humanVestedMINI<a.rows[1].humanMINI);
assert.throws(()=>forecast({supply:(U128_MAX/1000000n).toString(),years:1}),/u128/);
for(const bad of [{years:1001},{years:NaN},{supply:'abc'},{capturePct:-1},{activePct:0},{growthYears:1.5},{annualGuard:'yes'}])assert(validateLong({...longDefaults,...bad}).length);
const grow=forecast({years:1000,realPerCapitaGrowthPct:1,growthYears:100});
assert(Math.abs(grow.rows[1000].wealth/grow.rows[100].wealth-1)<1e-12);
// Constant purchasing-power target is an accounting counterfactual, never a policy activation.
for(const r of a.rows){assert(Math.abs(r.targetTotalMINI*a.rows[0].priceTotal-r.attributedWealth)<=1);}
const flat=forecast({populationPath:'current',years:1000});
assert(flat.rows.every(r=>r.targetTotalMINI===1e9&&r.targetNetIssuanceMINI===0));
assert(flat.rows[1000].buyingPowerIndex<1e-6);
assert.equal(zero.rows[0].targetTotalMINI,null);
const shrinking=forecast({populationPath:'decline',years:100});assert(shrinking.rows[100].targetNetIssuanceMINI<0);
const evidence={status:'passed',generated:'2026-09-12',scope:'Integer accounting, horizon boundaries, denominator identity, participation, Sybil leakage, vesting, dust, overflow, growth duration; not external audit or live economic evidence',summaries};
writeFileSync(new URL('./results.json',import.meta.url),JSON.stringify(evidence,null,2)+'\n');
const keys=['populationPath','annualGuard','elapsed','calendarYear','population','totalMicro','circulatingMicro','lockedMicro','humanMINI','serviceMINI','treasuryMINI','priceTotal','priceCirculating','humanAccruedPerEligible','humanAccruedUSD','spotHumanUSD','originalPassiveSharePct','buyingPowerIndex','targetTotalMINI','targetNetIssuanceMINI','targetNetIssuancePct','wealthRequiredForStableMINI','realWealthGapUSD'];
writeFileSync(new URL('./results.csv',import.meta.url),keys.join(',')+'\n'+summaries.flatMap(s=>s.rows.map(r=>keys.map(k=>r[k]??s[k]).join(','))).join('\n')+'\n');
console.log('Passed eight 1,000-year scenarios and boundary/adversarial checks.');
console.log(JSON.stringify(summaries.find(s=>s.populationPath==='stable'&&s.annualGuard).rows.map(r=>({year:r.calendarYear,population:r.population,supply:r.totalMINI,price:r.priceTotal,humanAnnualSpot:r.spotHumanUSD,humanAccrued:r.humanAccruedUSD})),null,2));
