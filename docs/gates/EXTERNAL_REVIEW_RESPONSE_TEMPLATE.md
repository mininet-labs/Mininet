# External review response template

Copy this file for a dated response to a Mininet external-review or
legitimacy gate. It is a handoff and closure record, not an audit by itself.
Keep the original scope package and preserve this response even when the gate
closes. A later review gets a new dated response rather than rewriting the
old one.

## 1. Review identity and exact state

- **Review ID:** `ER-YYYY-MM-DD-...`
- **Gate issue:** `#...`
- **Scope package:** `docs/gates/...`
- **Repository / Forge project:**
- **Exact commit, proposal, release, or object digest reviewed:**
- **Review period:**
- **Response publication:** public / private / redacted (explain why)

The reviewer may be anonymous or pseudonymous. Do not require legal-name
disclosure unless a separate lawful process requires it. Persistent continuity
can be recorded without publishing a legal identity.

## 2. Reviewer competence and independence

- **Relevant competence:**
- **Methods or standards used:**
- **Funding, employment, authorship, client, or close-control conflicts:**
- **Independence limitations:**
- **Would a different specialist be required for an excluded area?**

An anonymous reviewer may still provide technically weighty, reproducible
findings. This record must state what independence evidence is available and
what it does not prove.

## 3. Scope and exclusions

### Included

-

### Excluded

-

### Claims examined

| Claim or requirement | Exact evidence inspected | Result | Limitation |
|---|---|---|---|
| | | supported / partial / unsupported | |

Do not expand “tested,” “reviewed,” or “verified” into “audited,”
“production-ready,” “anonymous,” or “personhood-qualified.”

## 4. Review questions

Answer each question directly. Use `not applicable` with a reason rather than
leaving a material question blank.

1. Does the scope cover the claims made by the code, release, and front page?
2. Which claims are unsupported, over-broad, or missing an honest limitation?
3. Which attacks, failure modes, recovery cases, or scale conditions remain?
4. Does any path cross the voice/value wall or convert payment, employer,
   hardware, platform, or organization status into authority?
5. Are identity roots described honestly rather than as verified humans?
6. Are authorship, review, approval, canonicalization, release, installation,
   owner adoption, and compensation separate?
7. Can the reviewer reproduce the evidence from the exact state named above?
8. What must happen before Beta, value-bearing testing, or a production claim?

## 5. Findings

| ID | Severity | Exact location | Finding | Reproduction / evidence | Recommended action |
|---|---|---|---|---|---|
| F-001 | | | | | |

Preserve adverse findings, dissent, and uncertainty. Do not mark a finding
resolved because a new PR mentions it.

## 6. Response and disposition

Every material finding has one disposition:

- **Corrected:** link the exact change and verification.
- **Accepted with gate:** state the prohibition, owner, trigger, and evidence
  required before the gate can open.
- **Rejected:** cite the governing source and counter-evidence.
- **Deferred:** name the owner, condition, next action, and review date.
- **Constitutional disagreement:** identify the amendment or fork path; do not
  silently reinterpret the invariant.

| Finding | Disposition | Exact follow-up / owner | Review date | Evidence link |
|---|---|---|---|---|
| F-001 | | | | |

## 7. Closure recommendation

- **Recommended status:** closed / partially addressed / deferred / blocked
- **Why this status is justified:**
- **Prohibited claims that remain:**
- **Required release or owner-adoption conditions:**
- **Next reviewer or decision owner:**

Closure must be objective and scoped. A closed gate does not close adjacent
research, hardware, legal, or production gates. A partially addressed issue
stays open and should be shown as partial rather than silently removed.

## 8. Attestation

- **Reviewer or reviewing body:**
- **Signature / persistent key reference (optional):**
- **Date:**
- **Responsible Mininet decision owner:**
- **Owner/governance acceptance reference:**

External review supplies evidence. It does not grant the reviewer governance
authority, a vote, release power, custody, or owner-adoption power. AI review
is evidence only and carries zero approval weight.

## 9. What this response does not prove

- It does not prove a unique human behind an identity root.
- It does not prove production readiness outside the named scope.
- It does not replace external review in another cryptographic or legal domain.
- It does not create a forced update, hidden unmasking path, or canonical
  provider/team registry.
