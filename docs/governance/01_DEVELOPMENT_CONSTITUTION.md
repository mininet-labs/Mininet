# Mininet Development Constitution

**Status:** Normative development-governance interpretation for the bootstrap period  
**Version:** 0.2  
**Authority:** Subordinate to `SPEC-00`, frozen invariants, and accepted decisions  

> This document does not create a second Constitution. It translates Mininet's canonical constitutional rules into development-governance obligations. If it conflicts with `SPEC-00` or a frozen invariant, this document is wrong and must be amended.

## Preamble

Mininet exists to maximize individual sovereignty while minimizing mandatory trust. It is intended to let people create, communicate, collaborate, govern, exchange value, and operate infrastructure without permanent dependence on a company, government, founder, hosting provider, legal identity, or central administrator.

Mininet does not attempt to eliminate trust. It relocates trust toward evidence, cryptographic authorization, transparent process, continuity, and explicit owner consent.

This Constitution governs how Mininet itself evolves. It applies to code, specifications, research, cryptography, economics, documentation, governance, and tooling. It remains applicable whether development occurs on GitHub, Mininet Forge, or a future successor.

## Article I — Humanity before technology

Technology exists to serve people. People do not exist to serve a protocol, token, institution, or engineering ideal.

When elegance conflicts with freedom, freedom prevails. When speed conflicts with correctness, correctness prevails. When wealth conflicts with equal political voice, equal voice prevails. When convenience conflicts with sovereignty, sovereignty prevails.

## Article II — Participant sovereignty

A participant should retain meaningful control over identity, data, devices, relationships, subscriptions, moderation choices, governance participation, software adoption, and exit.

No participant must reveal more identity than is technically necessary for a voluntarily chosen role. Anonymous, pseudonymous, and public participation are all legitimate modes.

The protocol shall make actions verifiable without making real-world identity compulsory.

## Article III — Optionality

If a feature can remain optional without weakening protocol integrity, Mininet prefers optionality over enforcement.

Optional layers may include content filtering, moderation lists, safety warnings, recommendation systems, trust signals, identity attestations, AI assistance, subscriptions, and update timing.

Mandatory rules are justified only where required for cryptographic correctness, consensus safety, canonical settlement, constitutional governance, or interoperability. Optionality may not be used to make objective protocol truth subjective.

## Article IV — Identity, authority, reputation, and legitimacy

These concepts are independent:

- **Identity** describes the cryptographic or social continuity under which an action is made.
- **Authority** describes what that identity may do.
- **Reputation** describes accumulated evidence about previous conduct.
- **Legitimacy** describes why an action becomes canonical.

Legal identity is neither necessary nor sufficient for technical legitimacy. A pseudonym may earn durable reputation. An anonymous contributor may submit and be paid for accepted work. A public institution receives no special correctness privilege.

## Article V — Evidence before status

Claims are evaluated by evidence rather than title, wealth, fame, employer, seniority, or public identity.

Relevant evidence includes executable tests, invariant checks, formal reasoning, benchmarks, reproducible builds, provenance, independent review, adversarial analysis, interoperability results, external audits, and explicitly documented uncertainty.

Passing tests prove only what the tests cover. Experimental cryptography may not be described as production-safe solely because internal tests pass.

## Article VI — AI serves humanity

AI may propose, implement, test, explain, optimize, compare, attack, and review. AI output can constitute useful evidence.

AI may not independently establish constitutional legitimacy, cast a human governance vote, satisfy a human quorum, approve itself, or become an irreversible source of authority.

AI assistance must be attributable to a persistent proposal record and a human or governance process must accept responsibility for canonicalization. This requirement does not force public identity; responsibility may attach to a persistent pseudonymous governance identity.

## Article VII — Anonymous and pseudonymous contribution

Mininet shall permit contribution without compulsory real-world identification wherever protocol integrity allows.

An anonymous or pseudonymous participant may:

- submit proposals and evidence;
- receive review;
- build reputation under a persistent key if desired;
- receive compensation to a privacy-preserving address;
- choose later whether to link, rotate, abandon, or publicly identify that persona.

A role may require persistent cryptographic continuity, stake-independent reputation, threshold authorization, or conflict-of-interest disclosure. Such requirements must be narrowly tailored and must not be converted into compulsory legal identification.

## Article VIII — Voice/value separation

Money may buy resources, labor, storage, computation, bandwidth, or attention. Money may not buy political power.

Contributor compensation, bounties, grants, or treasury payments must not create governance weight. Governance rights must not be purchasable directly or indirectly through balances, hardware, contribution volume, or employer size.

## Article IX — Canonical history

Writing code does not change Mininet. Publishing a proposal does not change Mininet. Popularity does not change Mininet.

Mininet changes only when a proposal completes the applicable legitimacy process and enters canonical history.

Canonical history is defined by constitutional continuity, valid authorization, and verifiable evidence—not by a repository name, trademark, domain, company, or hosting platform.

## Article X — Free forks and earned legitimacy

Anyone may copy, modify, and distribute Mininet. Forking must remain technically and legally free.

A fork inherits code but not automatically the legitimacy, continuity, identity roots, governance history, treasury, release chain, or social trust of the canonical network.

Participants retain the right to exit, fork, refuse an update, or follow a different community. No centralized actor may prevent lawful technical exit.

## Article XI — Governed releases and owner adoption

A governed release is an eligible option, not a command.

No founder, maintainer, governance body, server, or network majority may force software activation on an owner's device. Adoption requires explicit owner approval naming the exact release or policy the owner has voluntarily chosen.

Owners may inspect, defer, reject, roll back where safe, remain on an older version, or deliberately follow a fork. Compatibility consequences must be stated honestly, never disguised as coercion.

## Article XII — Authority

Authority exists to protect integrity, not preserve officeholders.

All authority must be scoped, documented, auditable, revocable, and subject to succession. Emergency authority must expire or be reviewed. No authority becomes legitimate merely because it has existed for a long time.

## Article XIII — Founder bootstrap role

The founder is the temporary guardian of the project's constitutional direction while independent governance, maintainers, and self-hosted infrastructure are immature.

Founder authority is not an ownership claim over Mininet. Its purpose is to defend canonical history from premature capture, contradictory changes, unsafe releases, and erosion of frozen principles.

Founder authority should decrease only when replacement mechanisms demonstrably preserve or improve legitimacy. It must not be surrendered merely to perform decentralization theatrically, nor retained after safe succession is possible.

## Article XIV — Governance

Governance may determine canonical protocol changes, constitutional amendments, delegated authority, release eligibility, treasury use, and shared network rules.

Governance may not own participants, compel identity disclosure without necessity, force device adoption, erase the right to fork, sell political influence, or create an unreviewable authority.

## Article XV — Privacy is structural

Privacy shall rely on minimizing learned data, cryptographic protection, local control, and explicit disclosure—not promises by operators.

Development infrastructure should support anonymous discovery and contribution, pseudonymous continuity, selective disclosure, and private compensation. Logs and provenance should capture what is necessary to verify work without collecting unnecessary personal metadata.

## Article XVI — Simplicity and failure

Every dependency and protocol rule is a trust and maintenance cost. Security-critical systems should prefer established, reviewable primitives to fragile custom reinvention while isolating large dependencies behind narrow boundaries.

Systems must fail predictably. A compromised contributor, runner, mirror, maintainer, or platform must not silently rewrite canonical history or force adoption.

## Article XVII — Constitutional amendment

Constitutional amendment requires a higher threshold than ordinary code changes, explicit identification of affected directives and invariants, a public consideration period, adversarial review, and an immutable record of the prior wording.

No ordinary merge, release, founder preference, or emergency action may silently amend this Constitution.

## Constitutional review test

Every major proposal must answer:

1. Does it increase sovereignty without weakening integrity?
2. Does it minimize compulsory identity and data disclosure?
3. Does it preserve the voice/value wall?
4. Can its security claims be demonstrated?
5. Can the system survive loss or compromise of the proposing authority?
6. Does the weakest honest participant remain viable?
7. Is owner adoption still voluntary?
8. Can future contributors understand and continue the system without today's people or platforms?
9. Are anonymous, pseudonymous, and public participants treated according to evidence rather than status?
10. Would this leave a future child more free rather than less?
