# Start here — for a curious person

You don't need to be an engineer to understand what Mininet is for. This page
is written for you.

## The one-sentence version

Mininet is an attempt to build a version of the internet that **belongs to the
people who use it** — not to a company, a government, or whoever owns the
biggest servers.

## What that actually means for you

Today, the digital things that matter most in your life — your identity, your
messages, your money, your posts, your reputation — mostly live on systems
owned by someone else. They can change the rules, lock you out, sell your
data, hand it to a government, or simply switch off. You are a *user* of their
system. You do not own your place in it.

Mininet is built on a different bet: that the same things can be arranged so
that **no single party is in charge**, and so that the rules that protect you
can't be quietly changed later by whoever gets powerful.

Concretely, the design guarantees — structurally, in the code, not as a
promise — that:

- **Money can't buy political power.** People with more money can pay for more
  storage or fund more work, but they can never buy extra votes or control
  over the rules. Wealth and voice are walled apart on purpose.
- **Everyone's vote counts the same.** Not weighted by money, not weighted by
  who showed up first, not weighted by whose hardware is biggest.
- **Nobody can seize it, freeze it, or switch it off.** There is no owner, no
  admin key, no master password, no company that can be pressured into pulling
  a plug that doesn't exist.
- **Nobody can force software onto your device.** Updates are something you
  choose, never something pushed to you.
- **Nobody can unmask you.** Not the network, not a relay, not a
  "law-enforcement backdoor" — because there isn't one, by design.

## The honest part

Mininet is **not finished, and not safe to trust with real money or your real
identity yet.** The people building it say so loudly and everywhere — that
honesty is itself one of the project's rules. What exists today is a working
technical core and a lot of carefully-reasoned design; what's missing is
things like a phone app you could install, an independent security audit of
the money code, and answers to a few genuinely hard research problems that
nobody in the world has fully solved yet. Those gaps are listed openly, not
hidden.

If someone ever tells you Mininet is "done" and ready to hold your savings —
be skeptical, and check [`docs/STATUS.md`](STATUS.md) and
[`docs/gates/`](gates/), which track exactly what is and isn't ready.

## Why build it this way — the deeper "why"

Every hard choice in this project traces back to a small set of principles
about outlasting its own creators and never becoming the kind of power it was
built to escape. If you want to understand the *values* underneath everything,
read [`docs/FOUNDER_DIRECTIVES.md`](FOUNDER_DIRECTIVES.md) — it's written in
plain language, for exactly this purpose, and it's the one document the
builders consider more important than any line of code.

## Where to go next

- Curious about the vision in depth: [`docs/FOUNDER_DIRECTIVES.md`](FOUNDER_DIRECTIVES.md).
- Want to see what's genuinely built vs. still missing:
  [`docs/STATUS.md`](STATUS.md).
- Want to poke at whether the safety claims hold up:
  [`docs/AUDITOR_START.md`](AUDITOR_START.md).
- Want to build or run it yourself: [`docs/DEVELOPER_START.md`](DEVELOPER_START.md).
