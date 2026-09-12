/** Planning arithmetic in floating-point real reference euros and MINI; not consensus or payout code. */
export const defaults={basePay:48000,founderYears:3,founderFte:.75,expenses:15000,priorPaid:0,recognitionYears:5,leadFte:1,contributorFte:1,auditBudget:96000,contributors:6,councillors:6,councilFte:.25,onCostPct:20,opex:120000,annualIncome:1000000,openingCash:1500000,reserveMonths:18,futurePct:10,bridgeReserve:1000000,bridgeShock:0,supply:1000000000,price:.1,humans:100000,humanTarget:100000,years:200,serviceUse:100,treasuryUse:100,sybils:0,whalePct:10,whaleServicePct:0,providers:1000,providerCost:120,monthlyHumanTarget:50,valuation:'fixed-value',annualGuard:true,blackoutDays:21,localDailyCost:1000,liquidityHaircutPct:20,settlementDays:14};
export const reference='8d7dbe8512720ef2a61c4d2a94127fb8ddf66623';
export const source=p=>`https://github.com/mininet-labs/Mininet/blob/${reference}/${p}`;
export const milestones=[
 {id:'M0',name:'Document past work',fraction:.10,evidence:'Independent review of hours, deliverables, receipts and prior payments; conflicts declared. No self-approval.',horizon:'First 30 days of a funded programme',owner:'Independent compensation reviewers',gate:'Approved, funded retrospective offer'},
 {id:'M1',name:'Reproduce and hand over the forge',fraction:.20,evidence:'Three independent humans operate the forge and reproduce the release path; recovery and handoff recorded.',horizon:'Evidence gate, not a launch date',owner:'Forge maintainers + independent reviewers',gate:'Exact-state acceptance and challenge window'},
 {id:'M2',name:'Complete external review',fraction:.25,evidence:'Named external reviewers assess cryptography and economics; material findings are remediated and independently accepted.',horizon:'After external reviewers sign off',owner:'External specialists + release reviewers',gate:'Repository gates #47, #50, #72 and #93 remain independently controlled'},
 {id:'M3',name:'Demonstrate sustained operation',fraction:.25,evidence:'A 12-month pilot records availability, real resource costs, payment failures, reward concentration and privacy incidents.',horizon:'At least 12 months of measured operation',owner:'Operators + independent measurement group',gate:'Predeclared service targets and failure disclosure'},
 {id:'M4',name:'Complete founder-independent succession',fraction:.20,evidence:'Two exercised role/key rotations; founder unavailable drill; reproducible archives; three independent successors for each critical function.',horizon:'After a demonstrated handoff',owner:'Affected groups + independent constitutional review',gate:'Temporary authority ends; compensation never extends it'}
];
const finite=n=>Number.isFinite(n)&&n>=0;
export function validate(p){const errors=[];for(const [k,v] of Object.entries(defaults)){if(typeof v==='number'&&!finite(p[k]))errors.push(`${k} must be a finite non-negative number`);}for(const k of ['basePay','supply','price','humans','providers','recognitionYears'])if(!(p[k]>0))errors.push(`${k} must be greater than zero`);if(p.years<1||p.years>200||!Number.isInteger(p.years))errors.push('years must be an integer from 1 to 200');for(const k of ['serviceUse','treasuryUse','whalePct','whaleServicePct','liquidityHaircutPct','futurePct'])if(p[k]>100)errors.push(`${k} cannot exceed 100`);if(p.futurePct>=100)errors.push('futurePct must be below 100');if(p.liquidityHaircutPct>=100)errors.push('Liquidity haircut must be below 100');if(p.founderFte>1||p.councilFte>1||p.leadFte>1||p.contributorFte>1)errors.push('FTE cannot exceed 1');if(p.humanTarget<1)errors.push('humanTarget must be at least 1');for(const k of ['humans','humanTarget','sybils','contributors','councillors','providers'])if(!Number.isInteger(p[k]))errors.push(`${k} must be an integer`);if(p.whalePct>100)errors.push('whale percentage cannot exceed 100');return errors;}
export function compensation(p){
 const founder=p.basePay*1.5*p.leadFte,contributor=p.basePay*p.contributorFte,senior=p.basePay*1.5,council=p.basePay*1.2*p.councilFte,audit=p.auditBudget;
 const wages=founder+p.contributors*contributor+p.councillors*council;
 const oncost=wages*p.onCostPct/100,earnedPool=wages+audit,essential=earnedPool+oncost+p.opex;
 const historicLabor=Math.max(0,p.founderYears*p.founderFte*p.basePay*1.5-p.priorPaid);
 const recognitionLabor=Math.min(historicLabor,p.basePay*4),recognition=recognitionLabor+p.expenses;
 const recognitionCost=recognition+recognitionLabor*p.onCostPct/100;
 const costFactor=recognition>0?recognitionCost/recognition:1;
 const targetAnnual=recognition/p.recognitionYears,annualAwardCost=targetAnnual*costFactor;
 const reserveFloor=essential*p.reserveMonths/12;
 const awardFunded=p.openingCash>=reserveFloor+recognitionCost;
 const openingEscrow=awardFunded?recognitionCost:0;
 const openingOperating=p.openingCash-openingEscrow;
 const paidEssential=Math.min(openingOperating+p.annualIncome,essential);
 const essentialShortfall=essential-paidEssential;
 const afterEssential=openingOperating+p.annualIncome-paidEssential;
 const futureTarget=p.annualIncome*p.futurePct/100;
 const futureTransfer=Math.min(futureTarget,Math.max(0,afterEssential-reserveFloor));
 const pastPayment=awardFunded?Math.min(targetAnnual,earnedPool*.1):0;
 const pastCost=pastPayment*costFactor;
 const escrowRemaining=openingEscrow-pastCost;
 const closingCash=afterEssential-futureTransfer;
 const externalSpend=paidEssential+pastCost;
 const conservation=p.openingCash+p.annualIncome-externalSpend-futureTransfer-closingCash-escrowRemaining;
 const neededIncome=essential/(1-p.futurePct/100);
 const recognitionFundingNeeded=Math.max(0,reserveFloor+recognitionCost-p.openingCash);
 const plannedSurplus=p.annualIncome-essential-futureTarget;
 const netBurn=Math.max(0,essential-p.annualIncome),runway=netBurn>0?openingOperating/netBurn*12:null;
 return {founder,contributor,senior,council,audit,wages,oncost,earnedPool,essential,historicLabor,recognitionLabor,recognition,recognitionCost,costFactor,annualAwardCost,targetAnnual,reserveFloor,futureTarget,futureTransfer,pastPayment,pastCost,closingCash,externalSpend,essentialShortfall,neededIncome,plannedSurplus,runway,conservation,recognitionDeferred:targetAnnual-pastPayment,roleRatio:contributor>0?founder/contributor:0,founderWithPast:founder+pastPayment,founderPctOfSpend:externalSpend?(founder+pastPayment)/externalSpend*100:0,awardFunded,openingEscrow,escrowRemaining,recognitionFundingNeeded,openingOperating};
}
export function cashProjection(p,years=20){const c=compensation(p);let cash=c.openingOperating,future=0,escrow=c.openingEscrow;const rows=[];for(let year=1;year<=years;year++){const paid=Math.min(cash+p.annualIncome,c.essential);const remaining=cash+p.annualIncome-paid;const futureTransfer=Math.min(c.futureTarget,Math.max(0,remaining-c.reserveFloor));const payCost=Math.min(escrow,c.targetAnnual*c.costFactor,c.earnedPool*.1*c.costFactor);cash=remaining-futureTransfer;future+=futureTransfer;escrow-=payCost;rows.push({year,cash,future,escrow,pastRemaining:escrow/c.costFactor,recognitionPaid:payCost/c.costFactor,shortfall:c.essential-paid,reserveFloor:c.reserveFloor});}return rows;}
export function simulate(p){
 const errs=validate(p);if(errs.length)throw new Error(errs.join('; '));
 let supply=p.supply,circulating=p.supply,locked=0,issuedAll=0,whale=p.supply*p.whalePct/100,sybilBalance=0;
 const positions=[];const rows=[];const monthly=[];let prevC=circulating;
 const originalValue=p.supply*p.price;
 let maxConservation=0,maximumAnnual=0;
 for(let year=1;year<=p.years;year++){
  const opening=circulating;let humanIssued=0,serviceIssued=0,treasuryIssued=0,humanPer=0,humanVested=0;
  // Population reaches the selected target over 50 years and then stabilizes. This is an explicit scenario.
  const actualHumans=Math.max(1,Math.round(p.humans*Math.pow(p.humanTarget/p.humans,Math.min(year-1,50)/50)));
  const eligible=actualHumans+p.sybils;
  for(let month=0;month<12;month++){
   const t=(year-1)*12+month;
   const h=circulating*.02/12;
   let s=circulating*.0075/12*p.serviceUse/100;
   let tr=circulating*.0025/12*p.treasuryUse/100;
   if(p.annualGuard){s=Math.min(s,Math.max(0,opening*.0075-serviceIssued));tr=Math.min(tr,Math.max(0,opening*.0025-treasuryIssued));const room=Math.max(0,opening*.03-humanIssued-serviceIssued-treasuryIssued-h);if(s+tr>room){const f=room/(s+tr);s*=f;tr*=f;}}
   humanIssued+=h;serviceIssued+=s;treasuryIssued+=tr;humanPer+=h/eligible;
   sybilBalance+=h*p.sybils/eligible;
   whale+=s*p.whaleServicePct/100; // Share of service earnings, not a share of voting power.
   positions.push({amount:h,start:t+1,duration:12,channel:'human'},{amount:tr,start:t+1,duration:90/(365/12),channel:'treasury'});
   // Service compensation is liquid immediately at this envelope layer; evidence-specific delay omitted.
   issuedAll+=h+s+tr;supply=p.supply+issuedAll;
   let newLocked=0;for(const v of positions){const remaining=1-Math.max(0,Math.min(1,(t+1-v.start)/v.duration));newLocked+=v.amount*remaining;}
   locked=newLocked;prevC=circulating;circulating=supply-locked;
   // Total Human Share vesting released this month, including grants to previous cohorts.
   let vh=0;for(const v of positions){if(v.channel!=='human')continue;const before=Math.max(0,Math.min(1,(t-v.start)/v.duration));const after=Math.max(0,Math.min(1,(t+1-v.start)/v.duration));vh+=v.amount*(after-before);}humanVested+=vh;
   maxConservation=Math.max(maxConservation,Math.abs(supply-circulating-locked));
   if(year===1)monthly.push({month:month+1,humanAccrued:h/eligible,humanVested:vh/eligible,circulating,locked});
   // Remove fully vested positions: issuedAll retains their value, balances remain in circulating supply.
   for(let i=positions.length-1;i>=0;i--)if(t+1-positions[i].start>=positions[i].duration)positions.splice(i,1);
  }
  const value=p.valuation==='fixed-price'?circulating*p.price:p.valuation==='per-person'?originalValue*actualHumans/p.humans:originalValue;
  const price=value/circulating;
  const inflation=(humanIssued+serviceIssued+treasuryIssued)/opening*100;maximumAnnual=Math.max(maximumAnnual,inflation);
  rows.push({year,totalSupply:supply,circulating,locked,humans:actualHumans,eligible,humanIssued,serviceIssued,treasuryIssued,humanPer,humanVested,price,quotedValue:value,humanEuro:humanPer*price,whalePct:whale/supply*100,sybilPct:sybilBalance/supply*100,annualGrossPct:inflation,channelTotal:humanIssued+serviceIssued+treasuryIssued});
 }
 return {rows,monthly,maxConservation,maximumAnnual};
}
export function summary(p){const c=compensation(p);const annualHuman=p.supply*.02/(p.humans+p.sybils);const service=p.supply*.0075*p.serviceUse/100;const providerTokens=service*.5/p.providers;const providerGross=providerTokens*p.price,providerNet=providerGross-p.providerCost;const requiredValuation=p.monthlyHumanTarget*12*(p.humans+p.sybils)/.02;
 const bridgeLoss=p.bridgeReserve*(p.bridgeShock===1?.5*.8:p.bridgeShock===2?.4*.8:p.bridgeShock===3?.9*.8:p.bridgeShock===4?.1:0);
 return {c,annualHuman,humanMonthEuro:annualHuman*p.price/12,service,providerTokens,providerGross,providerNet,requiredValuation,requiredPrice:requiredValuation/p.supply,bridgeLoss,bridgeAfter:p.bridgeReserve-bridgeLoss,offlineBuffer:p.localDailyCost*(p.blackoutDays+p.settlementDays)/(1-p.liquidityHaircutPct/100),humanSybilLeak:p.supply*.02*p.sybils/(p.humans+p.sybils),quotedValue:p.supply*p.price};}
export const money=n=>new Intl.NumberFormat('en-IE',{style:'currency',currency:'EUR',maximumFractionDigits:Math.abs(n)<1?4:0}).format(n);
export function csv(rows){if(!rows.length)return '';const keys=Object.keys(rows[0]);return keys.join(',')+'\n'+rows.map(r=>keys.map(k=>r[k]).join(',')).join('\n');}
