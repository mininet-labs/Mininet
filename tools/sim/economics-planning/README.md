# Offline economics planning prototype

Open `index.html` directly in a modern browser, including Microsoft Edge on Windows. No account, server, package installation or network is needed for the calculations. Source links need internet; sibling reports open from a full repository checkout. This review artifact is separate from the requested Windows Mininet client.

All balances in `generations.mjs` are integer micro-MINI. Approximate USD outputs are scenarios, not consensus, prices, redeemable claims or a monetary-policy activation. `compensation.mjs` is a separate floating-point reference-EUR operating plan and historical 200-year exploratory model. Its old price denominator is circulating supply; the new wealth-interest model uses total issued supply and explicitly exports both quotations.

From the repository root, with Node.js 22 or later:

```sh
node tools/sim/economics-planning/check-compensation.mjs
node tools/sim/economics-planning/check-generations.mjs
node tools/sim/economics-planning/build.mjs
node tools/sim/economics-planning/check-ui.mjs
```

The generation check writes deterministic `results.json` and `results.csv`. The build embeds the model and population extract in `index.html`. Edit `review.html`, not the generated file. `population.json` preserves data provenance and a digest of the official source workbook; `population.mjs` is its JavaScript wrapper. Values are annual July World populations, WPP 2024 medium variant, converted from thousands to people. Post-2100 scenarios are our assumptions, not UN projections.

Inputs and scenario export are local. There is no persistence unless the user exports results. No external chart library, analytics, wallet, keys, executable node commands or payment path is included. Generated HTML is intentionally checked in so review works when Node/npm/GitHub are unavailable.

See `docs/proposals/economics-generations.md` and `docs/proposals/economics-compensation.md` for assumptions, findings, traceability, adverse cases and decisions requiring review. No Rust, ledger or governance activation files are changed.
