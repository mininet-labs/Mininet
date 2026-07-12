# Governance Failure Modes and Continuity Requirements

**Status:** Normative resilience catalogue  
**Version:** 0.3

## 1. Purpose

Governance is incomplete unless it explains how Mininet continues when trusted people, infrastructure, or assumptions fail. Each scenario below defines the required safe property, not a promise that the present implementation already satisfies it.

## 2. Founder unavailable

**Threat:** founder loses keys, is unreachable, resigns, or dies.  
**Required property:** canonical development, release verification, treasury custody, and authority succession can continue under previously defined recovery and delegation rules.  
**Bootstrap action:** maintain at least two tested recovery paths, document authority inventory, and avoid single-person secrets where continuity is required.  
**Forbidden response:** inventing an undocumented administrator or bypassing owner sovereignty.

## 3. GitHub unavailable or hostile

**Threat:** outage, account suspension, repository deletion, compromised organization owner, or coerced platform action.  
**Required property:** source, issues, proposal evidence, identities, release history, and governance continuity can be reconstructed from independent copies.  
**Bootstrap action:** regular signed mirrors and export drills.  
**Target:** Forge canonical history makes GitHub replaceable.

## 4. Forge compromised

**Threat:** storage corruption, malicious indexing, withheld objects, forged UI presentation, or implementation exploit.  
**Required property:** content digests, signatures, independent replication, and local verification expose corruption; users can continue from valid history or fork.  
**Forbidden response:** accepting server presentation as authority.

## 5. AI-generated malicious change

**Threat:** subtle backdoor, fabricated proof, poisoned tests, prompt injection, or coordinated AI agreement.  
**Required property:** AI cannot satisfy authorization quorum; exact-state human or authorized review, adversarial tests, provenance, and integration checks remain required.  
**Additional control:** reviewers must inspect claims and evidence, not the confidence or eloquence of the model.

## 6. Maintainer collusion or capture

**Threat:** maintainers approve malicious code, suppress alternatives, or trade authority for money.  
**Required property:** scoped authority, independent review, transparency, revocation, term review, fork freedom, and separation between wealth and political voice.  
**Signal:** unusual approval clusters, self-dealing, hidden conflicts, and bypass use trigger audit.

## 7. Working group capture

**Threat:** one domain becomes closed, stagnant, or privately controlled.  
**Required property:** public charter, open proposal intake, appeal path, periodic renewal, succession, split/merge/retire process, and cross-group integration review.

## 8. Treasury compromise

**Threat:** signing-key theft, collusion, invalid payment, or censorship of anonymous claimants.  
**Required property:** threshold custody, transparent authorization, bounded emergency pause only if constitutionally permitted, independent audit, and no conversion of treasury weight into governance weight.  
**Honesty:** current FROST and value constructions remain subject to external audit gates.

## 9. Release key compromise

**Threat:** attacker signs a malicious release.  
**Required property:** threshold authorization, provenance quorum, transparency, rollback/freeze protection, revocation, and local owner approval prevent one key from silently forcing activation.

## 10. CI or builder compromise

**Threat:** false green checks, altered artifacts, dependency substitution, or hidden network access.  
**Required property:** signed execution results, independent reproducibility, pinned environment, exact source binding, and policy that distinguishes trusted sandboxed steps from untrusted native execution.

## 11. Personhood mechanism failure

**Threat:** Sybil capture, false exclusion, privacy leakage, or coercive identity dependence.  
**Required property:** identity roots are not mislabeled as verified humans; political mechanisms depending on personhood remain gated until evidence is sufficient; optional signals do not become universal surveillance.

## 12. Network or community fork

**Threat:** irreconcilable constitutional disagreement.  
**Required property:** code and data continuity remain technically forkable; each side clearly identifies its history and authority; neither can counterfeit the other's legitimacy. Owners choose which continuity to follow.

## 13. Regulatory or infrastructure coercion

**Threat:** host, payment service, domain, or public maintainer is compelled to censor or reveal data.  
**Required property:** no single party can unmask all users, stop all development, force updates, or erase independently held history. Optional external compliance services must remain separable from core protocol sovereignty where technically possible.

## 14. Contributor disappears mid-work

**Threat:** abandoned branch, unavailable context, or inaccessible credentials.  
**Required property:** content-bound proposals, reproducible environments, documented acceptance criteria, and transferable issue ownership permit continuation without impersonation.

## 15. Governance deadlock

**Threat:** quorum cannot be reached or groups veto indefinitely.  
**Required property:** defer, expire, mediation, appeal, or fork paths exist. Deadlock MUST NOT silently authorize a proposal, and emergency paths MUST NOT become ordinary shortcuts.

## 16. Disaster-drill cadence

During GitHub bootstrap, the owner SHOULD conduct quarterly drills for:

- restoring repository and issues from export;
- rotating a compromised maintainer key;
- recovering founder access;
- rebuilding an artifact from pinned source;
- rejecting a forged or stale release;
- continuing a proposal through an integration branch after one engineer disappears.

After Forge becomes canonical, equivalent drills MUST use Forge-native objects and independent nodes.
