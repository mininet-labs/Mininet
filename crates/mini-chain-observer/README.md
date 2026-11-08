# mini-chain-observer

Read-only observation of external-chain (XRP Ledger, Monero) transfers at a
replaceable edge. Research PR #429, P2-1; decision D-0550.

It answers one question: *how sure are we that this external transfer happened?*
Callers feed it reports from independently operated sources; it weighs them.

- Holds **no keys**, signs nothing, mints nothing, and has no network code.
- Strongest output is `Status::Finalized`: evidence for a separately reviewed
  process, never a credit.
- Finalized needs enough **distinct** allow-listed sources agreeing on the same
  inclusion block with the profile's confirmations (XRPL also needs a validated
  ledger). Disagreement on amount, destination, asset or block is `Ambiguous`.
- Wrong network, unknown source, stale and future-dated reports are refused.
- A durable, idempotent receipt journal records the strongest honest status; a
  finalized receipt later contradicted is marked forever, never erased.

Not audited. A lying quorum is indistinguishable from an honest one, so source
independence is an operator question. Monero payments are private by design:
observing one needs a view-key or transaction-proof model that is out of scope.
