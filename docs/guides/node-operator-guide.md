# Running a Mininet node — the operator's guide

**Status: Day 0.** Everything below is real, checked-in, and runnable, but the appliance profile has not yet been run end-to-end on a fleet of real machines, and Mininet itself is prototype software under an external-audit gate ([#72](../../issues/72), D-0047). Do not put value you cannot afford to lose behind it. See `docs/design/mininet-node-appliance.md` (D-0446) for the staged plan this is stage one of.

This guide is for a person with a spare machine, not a datacenter operator. If you can install Debian and use a terminal, you can run a node.

---

## What a node is, and what running one means

A Mininet node is a machine that stores and serves content for the network and syncs with peers. It is not a mining rig and it does not earn by being large — the design deliberately gives a Raspberry Pi and a rack server the same governance weight, which is zero, because **money never buys voice** (Directive 16).

What running a node actually does:

- **holds and serves data** for people who are not you, which is the contribution;
- **carries an identity** — a `did:mini` key pair that is yours alone;
- **needs no permission** from anyone, including us. Nobody can turn it off remotely, and no update can be pushed to it (P3, U1).

What it does **not** do:

- earn you a vote. There is none to earn.
- make you anonymous by itself. Network-level privacy is a separate layer and is not finished.
- protect you from someone with physical access to the machine.

## Before you start: the one thing people get wrong

Your node's identity lives in `/var/lib/mininet`. **There is no recovery.** Not by us, not by a support team, not by any quorum — Mininet has no custodial recovery anywhere in it, by design (ID1), because a system that can restore your keys is a system someone else can be compelled to use against you.

The consequence is blunt: **if that disk dies and you have no backup, that identity is gone forever.** Cheap hardware is exactly the hardware whose storage fails. Make a backup on day one, before you have anything worth losing. It takes one command and it is the last section of this guide — read it first if you read nothing else.

## Hardware

| | minimum | comfortable |
|---|---|---|
| architecture | amd64 or arm64 | either |
| RAM | 900 MB | 2 GB |
| free disk on `/var` | 8 GB | 64 GB+ |
| network | any always-on connection | unmetered |

A Raspberry Pi 4 with a decent SD card or, better, a USB SSD clears the minimum. SD cards wear out — if you use one, the backup section is not optional.

Debian Stable is the supported base. Other distributions may work; nothing checks them and nothing is tested on them.

## Install

```bash
git clone https://github.com/mininet-labs/Mininet.git
cd Mininet

# Changes nothing. Tells you whether this machine will work.
deploy/installer/preflight.sh

# If preflight is happy:
sudo deploy/installer/install.sh
```

Preflight exits `0` when ready, `2` when installable but with advisories worth reading, and `1` when something is genuinely wrong — in which case `install.sh` refuses to start rather than getting halfway.

The installer is safe to re-run. Every step checks current state first, so running it again after you change a manifest applies only what changed.

### What it changes on your machine

Nothing outside these paths:

| path | what |
|---|---|
| `/usr/local/bin/mini` | the CLI |
| `/usr/local/lib/mininet/verify.sh` | self-check script the timer runs |
| `/var/lib/mininet` | **your identity and node state** |
| `/etc/mininet/appliance.conf` | your config (never overwritten once it exists) |
| `/etc/systemd/system/mininet*` | the units |
| `/etc/systemd/journald.conf.d/mininet-privacy.conf` | log retention |
| `/etc/sysctl.d/99-mininet-hardening.conf` | kernel hardening |
| `/etc/nftables.d/mininet.nft` | firewall rules |
| `/etc/sysusers.d/mininet.conf` | the `mininet` system user |

**Read the firewall file before you install.** `deploy/nftables/mininet.nft` begins with `flush ruleset` — it takes ownership of the machine's entire nftables policy. That is right for a dedicated node and wrong for a machine already running other firewalled services. If this box does other jobs, adapt the ruleset first.

### Start it

```bash
sudo systemctl start mininet.target
systemctl status mininet-sync-listen.service
```

Or pass `--start` to the installer. Otherwise the units are enabled and come up at next boot.

## Day-to-day

```bash
# Is the appliance layer intact?
sudo deploy/verification/verify.sh

# What has the node been doing?
journalctl -u mininet-sync-listen.service -n 50

# Stop / start
sudo systemctl stop mininet.target
sudo systemctl start mininet.target
```

A timer runs the same verification daily and records the result in the journal. It **reports and never repairs** — infrastructure that silently reapplies its own configuration is infrastructure that overrides deliberate changes you made, which is the wrong default on a machine you own.

### Configuration

`/etc/mininet/appliance.conf`. Today it holds one setting, the sync listen address. If you change the port, change it in `deploy/nftables/mininet.nft` too — the two are not coupled automatically, and a mismatch means a node that listens on a port the firewall drops.

```bash
sudo systemctl restart mininet-sync-listen.service   # after editing
```

### Logs and what they contain

Journal retention is deliberately short — 200 MB, two weeks, no debug records. A connection log is a record of **who talked to your node and when**, and an indefinitely-retained one is a deanonymization corpus sitting on your disk for whoever eventually gets the machine: a thief, a border search, a subpoena, a resold drive.

You can raise the limits in `/etc/systemd/journald.conf.d/mininet-privacy.conf` while debugging. Lower them again afterward.

## Backup — do this now

```bash
sudo deploy/backup/backup.sh /media/your-usb-stick
```

You choose a passphrase. **It cannot be reset or recovered.** The archive is AES-256 encrypted and contains your identity.

Three rules:

1. **Keep a copy off this machine.** A backup that dies with the disk it protects is not a backup.
2. **Anywhere you store it can attack the passphrase offline.** An unencrypted copy on a cloud drive is a key handed to that provider. Choose the passphrase accordingly, and prefer somewhere you control.
3. **Do not restore onto a second machine while the first still runs.** Two machines signing as one identity is *equivocation* — the network cannot distinguish it from an attack, and it is the same failure shape the protocol's own fraud detection exists to catch.

Restoring:

```bash
sudo deploy/backup/restore.sh /media/your-usb-stick/mininet-node-<stamp>.tar.gz.gpg
```

It refuses to overwrite existing state without `--force`, verifies the archive's digest before asking for a passphrase (so a corrupt file is distinguishable from a wrong password), and makes you type a confirmation.

## Updating

Two different things, often confused:

**The operating system** is Debian's, and updating it is ordinary Debian work — `apt upgrade` on your schedule. Mininet does not manage it and never will; a protocol that could push OS updates to your machine would be a protocol with an off switch.

**Mininet itself** goes through the governed release path, not `git pull`:

```bash
mini release verify <release-id>     # check it before anything touches disk
mini installer stage <release-id>
mini installer preflight
mini installer activate <release-id> # requires your explicit approval, by id
mini installer health-check
mini installer rollback              # if it went wrong
```

`activate` requires an approval object naming the exact release — the software cannot activate itself, and a failed health check rolls back to whatever was already running rather than forcing forward (U1, D-0071).

The `mini` binary the installer built from your checkout is a **bootstrap only**: the first binary on a fresh machine has nothing to verify itself against. Everything after it should go through the path above.

## Removing it

```bash
sudo deploy/installer/uninstall.sh
```

Removes the software, the units, and the appliance's system configuration. **Your identity is kept** — reinstalling picks up where you left off.

```bash
sudo deploy/installer/uninstall.sh --purge-state
```

Also deletes `/var/lib/mininet`. This is irreversible and requires typing a confirmation phrase. Back up first.

"No off switch" means nobody can disable *your* node remotely. It has never meant you cannot remove software from your own machine — software you cannot uninstall owns the machine rather than serving it.

## Troubleshooting

**The service restarts constantly.** That is normal. `mini sync listen` serves one peer and exits by design; systemd restarts it to keep the node listening. There is no daemon, and the appliance layer supplies the supervision instead of adding one to the application.

**Peers cannot reach me.** Check, in order: the service is running; the port in `appliance.conf` matches `mininet.nft`; nftables actually loaded the rules (`sudo nft list table inet mininet_filter`); and your router forwards the port. The last one is the usual answer.

**`verify.sh` fails after a Debian upgrade.** An upgrade may have flushed nftables or reset unit state. Re-run `sudo deploy/installer/install.sh` — it is idempotent and reapplies only what drifted.

**The clock is wrong.** Fix it before anything else. Proof windows, claim expiry, and update-freshness checks all read the device clock, and a node with a wrong clock produces evidence nobody else agrees with. `timedatectl status` should say `NTP synchronized: yes`.

**Disk filling up.** Check `/var/lib/mininet` and `journalctl --disk-usage`. Storage is what a node contributes, so growth is expected; a hard cap belongs to the storage policy layer and is not wired into this profile yet.

## What this profile does not do yet

Stated plainly because you are trusting it with a machine:

- **Not run on real hardware end-to-end at scale.** Manifests are lint-verified (`systemd-analyze`, `nft --check`, `systemd-sysusers --dry-run`, `shellcheck`) and the scripts are reviewed, but Day 0 has not been through a real fleet.
- **No signed images.** Phase 2. You build from source today.
- **No A/B root or transactional updates.** Phase 3.
- **No automatic OS security updates.** Deliberate — see above.
- **No fleet management.** There is no console, and there will not be a central one.
- **No storage quota enforcement** in this layer.
- **amd64 and arm64 only.**

## Where to ask

Issues and discussion: <https://github.com/mininet-labs/Mininet/issues>. GitHub is a temporary operational surface, not the network's authority — the long-term home is Mininet governing itself.

If something here is wrong or unclear, that is a bug in the guide. Say so.
