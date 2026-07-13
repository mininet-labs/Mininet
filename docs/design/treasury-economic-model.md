# Treasury economic model — founder decision (D-0073)

Resolves the design question `docs/gates/economic-simulation-spec.md` was
gating for [roadmap #47](../../issues/47): what actually replaces the
whitepaper's original "BTC/XMR-to-MINI contribution mechanism" framing.
This document records the founder's answer as a design spec; it is not a
claim that the mechanism is simulated, audited, or implemented yet — see
"What remains open" at the end.

## Why this supersedes the BTC/XMR framing

D-0008 already established XRPL as Mininet's external settlement bridge
(SPEC-06), separately from #47's still-open BTC/XMR contribution-mechanism
question. Bitcoin has no required role going forward: it is disabled at
launch, and may never become a primary bridge, pricing reference,
governance asset, or mandatory reserve. The replacement is a two-network
model — XRPL (public, banking-adjacent liquidity) and Monero (private,
censorship-resistant liquidity) — plus a treasury-contribution mechanism
that is explicitly *not* the same transaction type as ordinary bridge
trading.

This follows the founder requirement that external capital may enter
Mininet and receive economic value, but may never purchase political
authority — i.e. it is an application of the voice/value wall (P1,
Directive 16) to the bridge/treasury layer specifically.

## 1. Four separate economic mechanisms, never conflated

| # | Mechanism | New MINI minted? | Gated by |
|---|---|---|---|
| A | Human Share | Yes — universal, equal, presence-conditioned | personhood only (see D-0074) |
| B | Network service rewards | Yes — concave, capped, delayed | verifiable useful work |
| C | Ordinary bridge trading (XRP/XRPL-asset/XMR ⇄ circulating MINI) | No — swap of already-circulating supply | market/liquidity pool |
| D | Treasury contributions | Yes — bounded | the epoch mechanism below |

The wallet UI must force an explicit choice between C (reversible market
trade) and D (irreversible treasury contribution) — a bridge deposit must
never silently become a contribution.

## 2. Bridge-liquidity reserve allocation

Target allocation of the dedicated bridge-liquidity reserve (this reserve
only — development funds, bounty pools, emergency funds, and ordinary
public spending are accounted separately):

- **50% native XRP**
- **40% XMR**
- **10% operational contingency** (approved XRPL-issued fiat/stable-value
  assets needed for settlement with exchanges/gateways/payment providers)
- **BTC: 0% by default.**

Operating bands: XRP 45–60%, XMR 35–50%, contingency 0–10%, BTC 0% unless
constitutionally activated (absolute ceiling 10% of the reserve by market
value if ever activated; may not reduce XMR below its private-liquidity
floor; may not become the MINI pricing unit; may not create a BTC
redemption promise; ordinary treasury governance cannot activate it —
requires a separate constitutional decision, its own D-number).

If market movement pushes an asset outside its band, the treasury does
not force-sell — it stops acquiring the overweight asset and directs
future liquidity operations toward gradual rebalancing.

**Why XRPL and XMR, not one or the other:** XRPL gives fast public
settlement, exchange/market-maker access, approved issued assets, and a
banking-adjacent route; XMR gives private settlement, censorship
resistance, permissionless access, and an exit route when regulated
gateways exclude participants. Neither is Mininet's consensus,
governance, identity authority, or monetary constitution — failure of
XRP, XRPL, XMR, an issuer, exchange, gateway, or bank must impair only
that route, never Mininet's internal ownership or governance.

## 3. No guaranteed redemption

MINI is not a redeemable receipt for treasury assets. The treasury may
support liquidity but does not promise every MINI can be returned for a
fixed amount of XRP/XMR/fiat — a guaranteed redemption right would turn
the treasury into a reserve bank with run risk and continuous-solvency
dependency. Users exit through available market liquidity, not a
protocol claim against a fixed fraction of reserves.

## 4. Treasury-contribution epochs

Contributions process in **monthly epochs**. Every contribution in the
same epoch gets the same final terms — no first-come advantage. Before
an epoch opens, the protocol publishes (and cannot alter once the epoch
starts): accepted assets, valuation sources, rate formula, contribution
capacity, reserve needs, confirmation requirements, safety spread,
vesting period. An oversubscribed epoch accepts every valid contribution
proportionally and returns excess value — no privileged queue into the
next epoch.

## 5. Contribution valuation

Deterministic **weighted-median time-weighted price** from independent
markets:

- at least 3 operationally independent price sources, spanning at least
  2 unrelated control domains;
- 30-day observation window;
- precommitted outlier-rejection formula;
- all arithmetic deterministic and integer-based;
- source roster changeable only via timelock;
- governance controls the bounded *methodology*, never individual prices
  or individual contributors' settlements; no vote may retroactively
  alter a completed epoch.

## 6. Reserve-protection spread and vesting

Contribution MINI is calculated at the epoch reference value **minus a
default 5% reserve-protection spread** (a contribution valued at 1,000
reference units receives MINI corresponding to 950 units) — a buffer
against volatility, oracle error, bridge costs, and mint-and-dump
arbitrage, not a fee to an operator.

Contribution-derived MINI **vests linearly over 90 days** and carries
**no governance weight** before, during, or after vesting.

## 7. Issuance ceilings

Treasury-contribution issuance is subordinate to the universal Human
Share (D-0074) and cannot become the dominant new-MINI source:

- absolute ceiling: **0.25% of circulating MINI per year**;
- monthly capacity = min of (a) 1/12 of the remaining annual 0.25%
  allowance, (b) 10% of trailing 90-day organic MINI trading volume, (c)
  the amount actually needed to restore reserve bands;
- unused capacity **expires** — no accumulation into a future mint
  allowance;
- **per-human cap:** no verified human may receive more than 1% of a
  monthly epoch's capacity (supplements, does not replace, the global
  cap); the limit attaches to the verified human privately — splitting
  one contribution across addresses does not increase capacity, and the
  human↔address relationship is never published.

## 8. Contribution receipts

No MINI mints until the external contribution is final under the
relevant external network's own rules. A valid receipt binds: network +
asset, unique tx id, destination vault, finalized amount, confirmation/
finality evidence, epoch, recipient MINI commitment, rate-history entry,
minted amount, and proof the external tx hasn't already been credited.

**Three separate security domains, never collapsed:** receipt
verification, treasury custody, and MINI issuance authorization. A
treasury signer cannot merely declare a deposit occurred — external
receipt verification must independently establish it. Repeated,
conflicting, reorganized, misdirected, unsupported-asset, or ambiguous
receipts fail closed.

## 9. Cellular treasury custody

No single globally catastrophic treasury key. External assets are
distributed across independent vault cells separated by asset, purpose,
liquidity venue, operational region, and custody committee — **no normal
vault holds more than 10%** of the relevant external reserve. A large
expenditure spanning several vaults is approved and executed in stages,
so compromise of one vault cannot expose the whole treasury.

**Custody separation, stated explicitly (D-0089, founder review's
`custody-separation` P0 item):** a bridge-specific vault's signer
committee and the general treasury's signer committee are always
disjoint sets of people, keys, and devices — no individual may hold a
seat on both. This was already implied by "separated by... custody
committee" above and by §10's rule that no single majority may control
"rate-source administration, receipt verification, custody signing, mint
authorization, and accounting" together; this paragraph exists so it is
never left to inference. One compromised bridge committee member must
never also be a treasury signer, and vice versa.

Each vault uses an audited, asset-compatible threshold-signature system.
Mininet's FROST implementation (D-0059/D-0060) may be used only where it
is cryptographically compatible with the specific external asset *and*
after external audit of the complete chain-specific integration (see
#93/`docs/gates/dkg-audit-scope.md`) — a generic FROST demonstration is
not proof that XRP or XMR custody specifically is safe.

## 10. Signer selection and rotation

Treasury signer eligibility comes from verified-human governance, never
MINI balance, contribution size, storage capacity, or liquidity
provision. Each vault: geographically/jurisdictionally diverse signer
group, threshold ≥ two-thirds, short overlapping terms, periodic
resharing, emergency replacement for unavailable participants, public
commitments to every authorized action, no permanent seats. A
contributor gets no preference toward becoming a signer, oracle
operator, rate reviewer, bridge administrator, finality participant, or
governance reviewer — rate-source administration, receipt verification,
custody signing, mint authorization, and accounting must not be
controlled by the same majority.

## 11. Governance controls — explicit allow/deny list

Human governance **may**: approve/remove supported XRPL assets, change
bounded oracle sources, adjust the 5% spread within a narrow
constitutional band, adjust reserve targets within their bands, suspend
a contribution asset, reduce issuance capacity, authorize emergency
evacuation of a compromised vault.

Human governance **may not**: award a preferential rate, increase the
current epoch's capacity after seeing contributions, create retroactive
settlement rules, grant contributors additional voice, promise fixed
redemption, exceed the annual ceiling via an "emergency" label, spend the
Human Share allocation, or convert treasury participation into political
rank. Emergency authority may pause activity; it may not rewrite
ownership or silently redirect funds.

## 12. Influence and recognition

A contributor may receive a factual, optional acknowledgment that a
contribution occurred. It cannot increase vote weight, improve proposal
placement, unlock governance rights, increase identity confidence,
improve finality-committee selection, bypass review, produce a superior
exchange rate, unlock private governance information, or create an
authority-carrying title. Recognizing generosity must never convert into
rule — this is the voice/value wall (P1) applied to social recognition,
not just formal vote weight.

## 13. Required economic stress tests (pre-mass-launch gate, not yet run)

1. XRP −80%; 2. XMR −80%; 3. major XRPL issuer freeze/default; 4. all
regulated XRPL gateways unavailable; 5. XMR exchange liquidity vanishes;
6. a contributor attempts to consume an entire epoch; 7. coordinated
contribution splitting across many addresses; 8. oracle sources moving
together (shared upstream feed); 9. a 30% temporary manipulated price;
10. reserves outside every target band simultaneously; 11. one vault
committee turns malicious; 12. one-third of signers unavailable; 13.
receipt-verifier/oracle collusion; 14. mint-and-dump after vesting; 15. a
simultaneous bridge run with no guaranteed redemption; 16. a century-scale
scenario where every original operator is gone.

**Pass condition:** none of the above may mint unlimited MINI, rewrite an
existing receipt, seize user balances, create governance power, expose
the entire treasury, or make Mininet dependent on one external
institution.

## What remains open

This is a design decision, not a completed audit. `docs/gates/
economic-simulation-spec.md` (#47/#50) still gates: building the
deterministic simulation harness, running it against §13's stress list,
external mechanism-design/tokenomics review of the calibration (the 50/
40/10 split, 5% spread, 0.25%/0.75%/2% ceilings — see D-0074 — and the
per-human 1% epoch cap are *founder-set starting parameters*, not
values a simulation has yet validated), external cryptographic audit of
the chain-specific FROST/XRP and FROST/XMR custody integrations (#93),
an external-receipt-verification design for XRPL and XMR, and
representing bridge swaps vs. contribution minting as genuinely distinct
transaction types in `mini-treasury`. #47 stays open, retitled, tracking
exactly that remaining work.
