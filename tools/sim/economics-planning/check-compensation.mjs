import assert from 'node:assert/strict';
import {defaults,summary,simulate,compensation,cashProjection} from './compensation.mjs';
const base=summary(defaults),c=base.c;
assert.equal(c.founder,72000);assert.equal(c.council,14400);assert.equal(c.recognition,177000);assert.equal(c.recognitionCost,209400);assert.equal(c.essential,751680);assert.equal(c.pastPayment,35400);assert.equal(c.escrowRemaining,167520);assert.equal(c.futureTransfer,100000);assert.equal(base.annualHuman,200);assert.equal(base.providerGross,375);
assert(Math.abs(c.conservation)<.001);
const poor=compensation({...defaults,openingCash:0,annualIncome:0});assert.equal(poor.pastPayment,0);assert.equal(poor.awardFunded,false);assert.equal(poor.closingCash,0);assert.equal(poor.essentialShortfall,poor.essential);
const stopIncome=compensation({...defaults,annualIncome:0});assert.equal(stopIncome.pastPayment,35400);assert.equal(stopIncome.futureTransfer,0);assert(stopIncome.closingCash<stopIncome.reserveFloor); // A funded vested award is protected even when new income stops.
const seed=compensation({...defaults,leadFte:.5,contributorFte:.5,contributors:2,councillors:3,councilFte:.1,auditBudget:20000,opex:12000,annualIncome:200000,openingCash:250000});assert.equal(seed.essential,153536);assert.equal(seed.recognition,177000);assert.equal(seed.pastPayment,0);assert.equal(seed.recognitionFundingNeeded,189704);
const flow=cashProjection(defaults);assert(Math.abs(flow[4].escrow)<.001);assert.equal(flow[5].recognitionPaid,0);const cashEnd=flow.at(-1);const paidCore=flow.reduce((n,r)=>n+c.essential-r.shortfall,0);const paidPast=flow.reduce((n,r)=>n+r.recognitionPaid*c.costFactor,0);assert(Math.abs(defaults.openingCash+20*defaults.annualIncome-paidCore-paidPast-cashEnd.cash-cashEnd.future-cashEnd.escrow)<.01);
const results=[];
for(const humanTarget of [1000,100000,1000000000,10000000000,20000000000])for(const serviceUse of [0,50,100])for(const treasuryUse of [0,100])for(const sybils of [0,10000]){
 const p={...defaults,humanTarget,serviceUse,treasuryUse,sybils};const r=simulate(p);assert(r.maximumAnnual<=3.00000001);assert(r.maxConservation<.01);for(const row of r.rows){assert(row.locked>=0&&row.circulating>=0);assert(Math.abs(row.totalSupply-row.circulating-row.locked)<.01);assert(row.whalePct<=100);}results.push({humanTarget,serviceUse,treasuryUse,sybils,maxAnnual:r.maximumAnnual,supply:r.rows.at(-1).totalSupply});
}
const linear=simulate({...defaults,years:1});assert.equal(linear.monthly[0].humanVested,0);assert(linear.monthly[11].humanVested>linear.monthly[1].humanVested);
const wealthy=simulate({...defaults,years:1,whalePct:90});assert.equal(wealthy.rows[0].humanPer,linear.rows[0].humanPer);
const attacked=summary({...defaults,sybils:10000});assert(attacked.annualHuman<base.annualHuman);assert(attacked.humanSybilLeak>0);
const ung=simulate({...defaults,annualGuard:false});assert(ung.maximumAnnual>3); // Exposes annual opening-base ambiguity; not silently called a strict annual cap.
const noOptional=simulate({...defaults,serviceUse:0,treasuryUse:0});assert(noOptional.rows.every(r=>r.serviceIssued===0&&r.treasuryIssued===0));
for(const shock of [1,2,3,4]){const x=summary({...defaults,bridgeShock:shock});assert.equal(x.c.essential,c.essential);assert.equal(x.c.recognition,c.recognition);assert(x.bridgeAfter>=0);}
assert(Math.abs(summary({...defaults,bridgeShock:3}).bridgeLoss-720000)<1e-6);
assert.equal(summary(defaults).offlineBuffer,43750);
console.log(JSON.stringify({checks:'passed',scenarios:results.length,defaultCompensation:c,defaultParticipant:base,defaultFirstYear:linear.rows[0],unGuardedMaximumAnnual:ung.maximumAnnual,year200:simulate(defaults).rows.at(-1)},null,2));
