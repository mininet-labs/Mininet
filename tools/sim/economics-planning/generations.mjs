import {populationData} from './population.mjs';

// Research prototype, not consensus code. Exact micro-MINI balances; approximate USD valuations.
export const U = 1000000n;
export const U128_MAX = (1n << 128n) - 1n;
export const longDefaults = {supply:'1000000000',years:1000,populationPath:'stable',
  wealth:500e12,capturePct:100,activePct:100,sybilPct:0,
  realPerCapitaGrowthPct:0,growthYears:100,servicePct:100,treasuryPct:100,annualGuard:true};
export function validateLong(p){
  const errors=[];
  if(!/^\d+$/.test(String(p.supply)) || BigInt(p.supply||0)<1n || BigInt(p.supply||0)*U>U128_MAX) errors.push('Opening supply must be positive whole MINI within u128 micro-MINI capacity.');
  for(const k of ['wealth','capturePct','activePct','sybilPct','realPerCapitaGrowthPct','growthYears','servicePct','treasuryPct','years']) if(!Number.isFinite(p[k])) errors.push(k+' must be finite.');
  if(p.wealth<0||p.wealth>1e18)errors.push('Reference wealth must be from 0 to 1e18 USD.');
  for(const k of ['capturePct','servicePct','treasuryPct'])if(p[k]<0||p[k]>100)errors.push(k+' must be from 0 to 100.');
  if(p.activePct<=0||p.activePct>100)errors.push('Active participation must be above 0 and at most 100%.');
  if(p.sybilPct<0||p.sybilPct>100)errors.push('Sybil claims must be from 0 to 100% of honest participants.');
  if(p.realPerCapitaGrowthPct < -2||p.realPerCapitaGrowthPct>2)errors.push('Real per-capita wealth growth must be from -2 to 2%.');
  if(!Number.isInteger(p.growthYears)||p.growthYears<0||p.growthYears>1000)errors.push('Growth duration must be an integer from 0 to 1000.');
  if(!Number.isInteger(p.years)||p.years<1||p.years>1000)errors.push('Horizon must be an integer from 1 to 1000.');
  if(!['stable','decline','expansion','current'].includes(p.populationPath))errors.push('Unknown population path.');
  if(typeof p.annualGuard!=='boolean')errors.push('Annual guard must be boolean.');
  return errors;
}
export function populationAt(year,path){
  const initial=populationData.rows[0].population;
  if(path==='current')return initial;
  if(year<=2100)return populationData.rows[year-2026].population;
  const terminal=populationData.rows.at(-1).population;
  const rate=path==='decline'?-.001:path==='expansion'?.002:0;
  return Math.max(1,Math.round(terminal*Math.pow(1+rate,year-2100)));
}
const min=(a,b)=>a<b?a:b;
const units=n=>Number(n)/1e6;
const utilization=n=>BigInt(Math.round(n*10000)); // 0.0001 percentage-point resolution.
export function forecast(input={}){
  const p={...longDefaults,...input};const errors=validateLong(p);if(errors.length)throw new Error(errors.join(' '));
  let total=BigInt(p.supply)*U, circulating=total,locked=0n,allIssued=0n;
  const initial=total,positions=[],rows=[];let maxAnnual=0,conservation=true;
  const people0=populationData.rows[0].population;
  function row(elapsed,human=0n,service=0n,treasury=0n,perHuman=0,vested=0n,opening=circulating,unminted=0n){
    const calendarYear=2026+elapsed,population=populationAt(calendarYear,p.populationPath);
    const active=Math.max(1,Math.floor(population*p.activePct/100));
    const sybils=Math.floor(active*p.sybilPct/100),eligible=active+sybils;
    const wealth=p.wealth*population/people0*Math.pow(1+p.realPerCapitaGrowthPct/100,Math.min(elapsed,p.growthYears));
    const attributedWealth=wealth*p.capturePct/100;
    const priceTotal=attributedWealth/units(total),priceCirculating=attributedWealth/units(circulating);
    return {elapsed,calendarYear,population,active,eligible,wealth,attributedWealth,
      totalMicro:total.toString(),circulatingMicro:circulating.toString(),lockedMicro:locked.toString(),
      totalMINI:units(total),circulatingMINI:units(circulating),lockedMINI:units(locked),
      humanMINI:units(human),serviceMINI:units(service),treasuryMINI:units(treasury),
      humanMicro:human.toString(),serviceMicro:service.toString(),treasuryMicro:treasury.toString(),
      openingMicro:opening.toString(),unmintedHumanRemainderMicro:unminted.toString(),
      priceTotal,priceCirculating,miniPerPerson:units(total)/population,
      humanAccruedPerEligible:perHuman,humanAccruedUSD:perHuman*priceTotal,
      // Mature-system annualized spot measure; differs from trailing annual grants and vesting.
      spotHumanUSD:0.02*units(circulating)/eligible*priceTotal,
      humanVestedMINI:units(vested),sybilHumanUSD:perHuman*sybils*priceTotal,
      grossPct:elapsed?Number(human+service+treasury)/Number(opening)*100:0,
      originalPassiveSharePct:Number(initial)/Number(total)*100,
      numericalResolutionUSD:priceTotal/1e6};
  }
  rows.push(row(0));
  for(let elapsed=1;elapsed<=p.years;elapsed++){
    const population=populationAt(2026+elapsed,p.populationPath);
    const active=Math.max(1,Math.floor(population*p.activePct/100));
    const eligible=BigInt(active+Math.floor(active*p.sybilPct/100));
    const opening=circulating;let hYear=0n,sYear=0n,tYear=0n,vestedYear=0n,unminted=0n,per=0;
    for(let m=0;m<12;m++){
      const t=(elapsed-1)*12+m;
      const cap=circulating/600n; // 2% / 12, floor, circulating at epoch opening.
      const each=cap/eligible,h=each*eligible;unminted+=cap-h;per+=units(each);
      let s=circulating*utilization(p.servicePct)/(1600n*1000000n);
      let tr=circulating*utilization(p.treasuryPct)/(4800n*1000000n);
      if(p.annualGuard){
        s=min(s,opening*75n/10000n-sYear);tr=min(tr,opening*25n/10000n-tYear);
        const room=opening*3n/100n-hYear-sYear-tYear-h;
        if(room<0n)throw new Error('Protected Human Share exceeds proposed total budget.');
        if(s+tr>room){const combined=s+tr;s=s*room/combined;tr=tr*room/combined;}
      }
      hYear+=h;sYear+=s;tYear+=tr;
      total+=h+s+tr;allIssued+=h+s+tr;
      if(total>U128_MAX)throw new Error('u128 total supply exceeded at elapsed year '+elapsed);
      positions.push({amount:h,start:t+1,channel:'human'},{amount:tr,start:t+1,channel:'treasury'});
      let nextLocked=0n;
      for(const v of positions){
        // 365-day policy years and 12 equal epochs; 90-day contribution vesting, start at epoch end.
        const denominator=v.channel==='human'?12n:1080n;
        const factor=v.channel==='human'?1n:365n;
        const age=BigInt(t+1-v.start),beforeAge=age>0n?age-1n:0n;
        const released=v.amount*min(denominator,age*factor)/denominator;
        const before=v.amount*min(denominator,beforeAge*factor)/denominator;
        nextLocked+=v.amount-released;if(v.channel==='human')vestedYear+=released-before;
      }
      locked=nextLocked;circulating=total-locked;
      conservation &&= total===initial+allIssued && total===circulating+locked;
      for(let i=positions.length-1;i>=0;i--){const v=positions[i];if((t+1-v.start)*(v.channel==='human'?1:365)>=(v.channel==='human'?12:1080))positions.splice(i,1);}
    }
    const r=row(elapsed,hYear,sYear,tYear,per,vestedYear,opening,unminted);maxAnnual=Math.max(maxAnnual,r.grossPct);rows.push(r);
  }
  // Counterfactual constant-purchasing-power schedule. This is NOT the adopted issuance policy.
  // Price is indexed to the opening basket-equivalent reference. No market peg is asserted.
  const initialPrice=rows[0].priceTotal;
  for(let i=0;i<rows.length;i++){
    const r=rows[i],previous=rows[Math.max(0,i-1)];
    r.buyingPowerIndex=initialPrice>0?r.priceTotal/initialPrice*100:null;
    r.targetBuyingPowerIndex=initialPrice>0?100:null;
    r.targetTotalMINI=initialPrice>0?r.attributedWealth/initialPrice:null;
    r.targetNetIssuanceMINI=i&&initialPrice>0?r.targetTotalMINI-previous.targetTotalMINI:0;
    r.targetNetIssuancePct=i&&initialPrice>0?r.targetNetIssuanceMINI/previous.targetTotalMINI*100:0;
    r.actualNetIssuanceMINI=i?r.totalMINI-previous.totalMINI:0;
    r.actualNetIssuancePct=i?r.actualNetIssuanceMINI/previous.totalMINI*100:0;
    r.realWealthGrowthPct=i&&previous.wealth>0?(r.wealth/previous.wealth-1)*100:0;
    r.wealthRequiredForStableMINI=p.capturePct>0?initialPrice*r.totalMINI/(p.capturePct/100):null;
    r.realWealthGapUSD=r.wealthRequiredForStableMINI===null?null:r.wealthRequiredForStableMINI-r.wealth;
  }
  return {assumptions:p,rows,maxAnnual,conservation};
}
