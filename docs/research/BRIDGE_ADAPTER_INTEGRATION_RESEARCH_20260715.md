Bridge Adapter Integration Research Report

Research target

Repository: mininet-labs/mininet
Track: Post-MN-207 bridge adapters
Question: Which external implementations should Mininet bind, vendor, or execute out-of-process for obfs4/Lyrebird, WebTunnel, Snowflake, and Tor pluggable transports?
Research date: 15 July 2026

⸻

Executive conclusion

Mininet should not vendor or rewrite the protocol implementations for obfs4, WebTunnel, or Snowflake at this stage.

The recommended integration model is:

Adapter	Initial integration	Later integration	Recommendation
obfs4 / Lyrebird	Managed external subprocess using Tor PT protocol	Optional narrow library extraction after audit	Ship first
WebTunnel	Managed external subprocess	Consider native adapter only after operational experience	Ship second
Snowflake	External standalone client or Tor-managed PT	Native integration only if broker/WebRTC lifecycle can remain upstream-compatible	Ship third
Tor compatibility	Connect to an existing Tor daemon through SOCKS/control boundary	Consider Arti as a separate future bearer	Keep externally isolated
Generic Tor PT support	Implement Tor PT v1 process manager	Retain as permanent compatibility layer	Core recommendation

The most important architectural decision is:

Mininet should own the adapter contract, process supervision, policy selection, bridge authentication, telemetry boundaries, and reproducible packaging—but should not own the censorship-circumvention protocol implementations yet.

The Tor Pluggable Transport specification is expressly designed around modular subprocesses that transform traffic and communicate with the parent application through a defined startup, shutdown, and IPC protocol. That architecture matches Mininet's need to gain transport diversity without linking the trusted protocol core to every PT implementation. (spec.torproject.org)

Lyrebird, WebTunnel, and Snowflake are active Tor Project codebases rather than merely papers or abandoned prototypes:

* Lyrebird has a maintained Tor Project repository with versioned tags and a BSD-2-Clause licence. (GitLab)
* WebTunnel is a Tor Project HTTP Upgrade-based pluggable transport with tagged releases and an MIT licence. (GitLab)
* Snowflake is a Tor Project WebRTC pluggable transport with a substantially larger code and release history and a BSD-3-Clause licence. (GitLab)

However, "maintained and deployed by Tor" must not be translated into "independently audited for Mininet's exact use." I found strong public evidence of active development, releases, operational deployment, and research scrutiny, but not enough public evidence in this pass to assert that every selected version has received a complete independent security audit covering Mininet's integration boundary.

Therefore Mininet should classify them as:

externally maintained, field-deployed circumvention implementations with upstream security responsibility, wrapped by a Mininet-owned sandbox and integration review.

The first deliverable should be a Tor PT v1 process adapter plus Lyrebird, not four native integrations at once.

⸻

1. Decision to be made

MN-207 established the bridge doctrine:

* transport agility;
* private and rotating bridges;
* obfs4/Lyrebird;
* WebTunnel;
* Snowflake;
* Tor PT compatibility;
* local Wi-Fi/Bluetooth forwarding;
* no single public directory.

The remaining implementation question is narrower:

How should Mininet consume mature external transports without importing unnecessary complexity or losing control of security-critical boundaries?

Four strategies are available:

1. Shell out to the upstream executable.
2. Use the upstream project as a managed subprocess through a structured protocol.
3. Bind its implementation as a library.
4. Vendor or reimplement its protocol inside Mininet.

These choices have different security implications.

⸻

2. The distinction between shelling out and managed subprocesses

"Shell out" is often used loosely.

Mininet should prohibit arbitrary shell execution while permitting a carefully constrained managed child process.

Unsafe shell execution

sh -c "<descriptor-provided command>"

This introduces:

* command injection;
* quoting ambiguity;
* environment expansion;
* PATH substitution;
* shell startup files;
* arbitrary executable selection;
* redirection;
* pipeline behaviour;
* platform differences.

This must be rejected.

Acceptable managed subprocess

execve(
    pinned_absolute_executable,
    fixed_argument_vector,
    minimal_environment,
    dedicated_working_directory
)

with:

* no shell;
* executable allowlist;
* verified binary digest;
* fixed protocol;
* bounded environment;
* closed inherited file descriptors;
* explicit stdin/stdout control channel;
* separate data sockets;
* lifecycle timeout;
* resource limits;
* structured errors;
* redacted logs.

The recommendation to use subprocess adapters refers only to the second model.

⸻

3. Why the Tor PT process protocol is the correct first boundary

The Tor PT specification defines pluggable transports as modular subprocesses and standardises how the parent and child coordinate startup, shutdown, and inter-process communication. (spec.torproject.org)

That gives Mininet a mature separation model:

Mininet transport router
        │
        │ structured PT control protocol
        ▼
external PT implementation
        │
        │ local loopback listener
        ▼
obfuscated network connection
        │
        ▼
Mininet bridge

Advantages

* PT implementation crashes do not corrupt Mininet memory.
* Go runtimes do not enter the main Rust process.
* Dependency trees stay outside consensus and identity crates.
* Upstream security updates can be adopted without translating code.
* Different PTs can use one supervision framework.
* The Mininet inner handshake remains independent.
* A compromised PT process can be sandboxed.
* Desktop deployment becomes practical before native mobile integration.

Disadvantages

* process startup latency;
* binary packaging;
* platform-specific sandboxing;
* IPC complexity;
* mobile operating systems may resist helper processes;
* the child process still sees bridge traffic locally;
* reproducible builds must include upstream binaries;
* version skew must be controlled.

Decision

Implement a strict subset of Tor PT v1 sufficient for client-side transports.

Do not initially implement every optional environment variable or server mode.

⸻

4. Recommended trust boundary

The PT process should be treated as network-facing and potentially compromised.

It is trusted to:

* transform bytes;
* connect to the configured bridge endpoint;
* report transport-level readiness.

It is not trusted to:

* authenticate the Mininet bridge;
* select the destination;
* choose privacy policy;
* access identity keys;
* read application objects before inner encryption;
* access capabilities;
* modify Mininet governance state;
* report final protection truth;
* select weaker fallback;
* write arbitrary files.

After the outer PT connection is established, Mininet performs its own authenticated inner bridge handshake.

Therefore even a malicious PT process cannot silently redirect the client to a fake Mininet bridge without the inner authentication failing.

⸻

5. General adapter architecture

5.1 Mininet-owned trait

pub trait BridgeAdapter {
    fn adapter_id(&self) -> BridgeAdapterId;
    fn capabilities(&self) -> AdapterCapabilities;
    async fn connect(
        &self,
        request: BridgeConnectRequest,
    ) -> Result<BridgeChannel, BridgeAdapterError>;
    async fn health_check(
        &self,
        request: BridgeHealthRequest,
    ) -> Result<BridgeHealth, BridgeAdapterError>;
}

5.2 External-process implementation

pub struct ManagedPtAdapter {
    executable: VerifiedExecutable,
    version_policy: PtVersionPolicy,
    sandbox_policy: PtSandboxPolicy,
    resource_policy: PtResourcePolicy,
    transport_name: TransportName,
}

5.3 Native implementation

A native adapter may later implement the same trait.

The privacy-policy router must not know whether a transport is:

* native Rust;
* Go library;
* managed subprocess;
* Tor daemon;
* remote proxy.

It consumes only typed capabilities and measured results.

⸻

6. Binary supply-chain policy

Running an external binary does not eliminate Mininet's supply-chain responsibility.

The project must own:

* accepted upstream repository;
* accepted version;
* source commit;
* build recipe;
* compiler/toolchain version;
* binary digest;
* licence record;
* patch policy;
* reproducibility result;
* provenance attestation;
* update procedure;
* rollback protection.

Recommended manifest:

pub struct ExternalAdapterManifest {
    pub adapter: BridgeAdapterId,
    pub upstream_project: UpstreamProjectId,
    pub upstream_commit: Digest,
    pub source_digest: Digest,
    pub build_recipe_digest: Digest,
    pub executable_digest: Digest,
    pub minimum_version: Version,
    pub maximum_tested_version: Version,
    pub protocol_version: PtProtocolVersion,
}

Mininet must never execute "whatever lyrebird appears first on PATH."

⸻

7. Audit terminology

The project should distinguish:

Protocol-reviewed

The design has academic or public technical analysis.

Upstream-maintained

The code is actively developed and receives releases.

Field-deployed

The implementation is used in real circumvention deployments.

Independently audited

A named third party reviewed a specific version or code range and published findings.

Mininet integration-reviewed

The adapter boundary, sandbox, configuration, downgrade handling, and inner authentication were reviewed for Mininet.

These categories are not interchangeable.

The public evidence reviewed here supports describing the Tor implementations as maintained and deployed. It does not justify an unconditional claim that every upstream component is fully independently audited for Mininet.

⸻

8. Lyrebird / obfs4

8.1 What it is

Lyrebird is the current Tor Project codebase carrying obfs4-family pluggable transport functionality. Its project is versioned and distributed under BSD-2-Clause terms. (GitLab)

8.2 Integration options

A. Managed Lyrebird subprocess

Mininet launches the upstream executable with:

* a fixed obfs4 transport request;
* a private state directory;
* bridge parameters from a signed descriptor;
* Tor PT control environment;
* loopback-only local listener.

B. Go library binding

Build Lyrebird components as a Go library and bind through cgo or generated C ABI.

C. Protocol reimplementation in Rust

Implement obfs4 directly inside Mininet.

8.3 Recommendation

Use the managed subprocess.

Reasons:

* preserves upstream implementation behaviour;
* avoids embedding a Go runtime in Mininet;
* avoids cgo memory and lifecycle complexity;
* makes upstream updates easier;
* limits compromise to a sandboxed process;
* supports one generic PT supervisor;
* avoids an unnecessary cryptographic and protocol rewrite.

8.4 Why not bind the Go library now

A Go library binding gives an illusion of tighter integration but creates:

* C ABI ownership ambiguity;
* callback and threading complexity;
* Go runtime inclusion;
* cross-compilation problems;
* difficult mobile packaging;
* more dangerous in-process parsing;
* tighter coupling to internal upstream APIs rather than the stable PT interface.

The subprocess interface is more stable than internal package APIs.

8.5 Why not rewrite obfs4

A rewrite would require reproducing:

* handshake behaviour;
* active-probing resistance;
* framing;
* replay controls;
* certificate handling;
* traffic distributions;
* edge-case error behaviour.

Even a wire-compatible implementation could have a distinct detectable fingerprint.

8.6 Deployment order

Lyrebird should be first because:

* it is narrower than Snowflake;
* it avoids browser/WebRTC brokerage;
* it exercises the generic PT manager cleanly;
* it provides a meaningful active-probe-resistant bridge mode;
* it gives Mininet an immediate censorship-resistant adapter without introducing HTTP camouflage configuration.

⸻

9. WebTunnel

9.1 What it is

The Tor Project describes WebTunnel as a pluggable transport based on HTTP Upgrade, with tagged releases under the MIT licence. (GitLab)

9.2 Integration options

A. Managed WebTunnel process

Use upstream WebTunnel as a PT child process.

B. Native Rust HTTP Upgrade adapter

Implement the transport directly over an existing Rust HTTP/TLS stack.

C. Reverse-proxy-only integration

Have Mininet speak ordinary WebSocket or HTTP Upgrade without the upstream WebTunnel client.

9.3 Recommendation

Use the upstream managed process first.

WebTunnel's security is not just "make an HTTP Upgrade request."

Operational behaviour matters:

* request shape;
* path configuration;
* TLS deployment;
* decoy website behaviour;
* reverse-proxy interaction;
* error handling;
* timing;
* server/client compatibility.

A home-grown HTTP tunnel might connect successfully while having a unique, easily classifiable fingerprint.

9.4 Native adapter threshold

A native Rust implementation should be considered only when:

1. upstream wire behaviour is clearly specified;
2. Mininet has packet-trace comparison tests;
3. client and server compatibility tests exist;
4. active probing has been evaluated;
5. HTTP/TLS fingerprinting is measured;
6. an external review covers the implementation;
7. mobile process restrictions make native integration necessary.

9.5 Server deployment

Mininet bridge operators should initially deploy the upstream WebTunnel server arrangement rather than modify it into a Mininet-specific public-facing protocol.

The Mininet bridge handshake begins only after WebTunnel carriage is established.

⸻

10. Snowflake

10.1 What it is

Snowflake is a Tor Project pluggable transport using WebRTC and inspired by Flashproxy. Its upstream repository has a much larger history of commits, releases, and operational components than the other two reviewed projects. (GitLab)

Tor documents standalone Snowflake proxy deployment for persistent servers and notes that unrestricted NAT and UDP availability improve reliability. (community.torproject.org)

Snowflake is not one executable performing one tunnel.

Its operational system contains multiple roles:

client
broker
volunteer proxy
bridge/server
NAT traversal infrastructure

10.2 Integration options

A. Use Snowflake through Tor

Mininet opens a Tor stream, while Tor Browser/Tor daemon owns Snowflake.

B. Run the upstream Snowflake client directly

The client obtains a volunteer proxy and connects toward a Mininet-compatible Snowflake server or bridge arrangement.

C. Embed Snowflake libraries

Bind its Go packages or reproduce them natively.

D. Build a Mininet-specific Snowflake network

Operate separate brokers, volunteer proxies, and servers.

10.3 Recommendation

For the first release:

Use Snowflake through the existing Tor ecosystem, not as a Mininet-native standalone network.

The initial Mininet adapter should therefore be:

Mininet
  → local Tor SOCKS
  → Tor-controlled Snowflake PT
  → Tor network
  → Mininet bridge/onion endpoint

or, where the PT supervisor supports it safely:

Mininet
  → managed upstream Snowflake client
  → existing Snowflake infrastructure
  → compatible bridge

10.4 Why Snowflake comes third

It introduces more dependencies and failure modes:

* broker reachability;
* WebRTC implementation;
* ICE;
* STUN/TURN assumptions;
* NAT variation;
* volunteer-proxy reliability;
* UDP restrictions;
* more complex telemetry;
* more complex server deployment.

Research has also shown that Snowflake's use of WebRTC did not automatically make it indistinguishable from ordinary WebRTC applications in the studied traces; handshake features permitted highly accurate classification in that analysis. (arXiv)

Therefore Mininet must call it:

an address-agile volunteer-proxy transport,

not:

indistinguishable ordinary WebRTC traffic.

10.5 Do not fork the ecosystem early

A separate Mininet Snowflake broker/proxy ecosystem would begin with:

* few proxies;
* low address diversity;
* weak operational monitoring;
* recognisable broker infrastructure;
* no mature volunteer base.

That discards the primary advantage Snowflake buys.

⸻

11. Tor compatibility adapter

11.1 Recommended boundary

Mininet should initially treat Tor as an external bearer reached through:

* SOCKS5;
* optionally a restricted control-port integration;
* an onion service or ordinary bridge destination.

Do not link Tor's full networking implementation into core Mininet crates.

11.2 Responsibilities Mininet retains

* destination selection;
* Mininet bridge authentication;
* protection-class decision;
* timeout;
* stream isolation credentials;
* achieved-policy reporting;
* prohibition on direct downgrade.

11.3 Responsibilities Tor retains

* circuit construction;
* bridge handling;
* PT execution;
* directory interaction;
* Tor network reachability;
* onion-service routing.

11.4 Stream isolation

Every Mininet privacy scope should use appropriately isolated SOCKS credentials or separate Tor isolation contexts.

Otherwise unrelated Mininet operations may share circuits and become linkable.

11.5 Control-port caution

The Tor control port is powerful.

Mininet should not require broad control access merely to open streams.

Where used, authenticate strongly and expose only a narrow internal wrapper.

⸻

12. Arti

Arti is relevant because it is Tor's Rust implementation, but it should be evaluated as a future Tor bearer, not as the solution to all PT integration.

Potential advantages:

* Rust;
* library-oriented architecture;
* easier in-process use;
* fewer C boundaries;
* future mobile suitability.

Potential concerns:

* PT feature parity;
* bridge and Snowflake support maturity;
* API stability;
* dependency weight;
* runtime integration;
* Tor-specific state management entering Mininet's process.

Recommendation:

Track Arti separately. Do not block the first bridge-adapter release on it, and do not assume Rust automatically makes an in-process dependency safer than an isolated mature executable.

A later design review should compare:

external Tor daemon
versus
embedded Arti client

for one specifically defined Mininet bearer.

⸻

13. Vendoring policy

"Vendor" has two meanings that should be separated.

13.1 Vendoring source for reproducible build

Mininet may retain an exact upstream source snapshot or verified source archive in its build system.

This can be acceptable when:

* provenance is recorded;
* upstream history remains linked;
* local patches are minimal;
* licence files are retained;
* security updates remain trackable;
* the source is built as a separate executable.

13.2 Forking and maintaining the implementation

Mininet becomes responsible for protocol development and security fixes.

This should be avoided.

Recommendation

Use:

pinned upstream source
→ reproducible external binary
→ verified digest
→ sandboxed execution

Do not create:

mininet-obfs4-fork
mininet-webtunnel-fork
mininet-snowflake-fork

unless upstream becomes unavailable and governance explicitly accepts long-term maintenance responsibility.

⸻

14. Dynamic downloading

The client should not download PT executables from arbitrary URLs at runtime.

Risks include:

* censorship substitution;
* compromised mirrors;
* downgrade;
* unsigned binaries;
* metadata leakage;
* inconsistent versions;
* execution before full provenance verification.

Recommended distribution:

* bundled in official Mininet release;
* separately packaged but release-attested;
* installed by system package manager from an approved package;
* verified through Mininet release/provenance tooling.

⸻

15. Sandbox requirements

A PT subprocess should run with:

Filesystem

* dedicated empty state directory;
* no repository access;
* no identity store;
* no wallet;
* no capability store;
* no SSH or Git credentials;
* read-only executable and libraries;
* bounded writable storage.

Network

The PT must connect only to:

* descriptor-authorised bridge endpoints;
* protocol-required broker/STUN/TURN services for transports such as Snowflake;
* configured local loopback addresses.

Transport-specific exceptions must be declared.

Process

* no child process creation unless explicitly required;
* CPU limit;
* memory limit;
* file-descriptor limit;
* process-count limit;
* startup timeout;
* idle timeout;
* termination grace;
* hard kill fallback.

Environment

* clean allowlist;
* no inherited proxy variables unless intentionally set;
* no user home path;
* no cloud credentials;
* no debugging secrets;
* no unrelated locale or host metadata where avoidable.

Logging

* stderr captured;
* bridge secrets redacted;
* no application payload;
* no identity DID;
* no capability;
* no destination beyond permitted operational endpoint metadata;
* rotation and deletion policy.

⸻

16. Packaging models

Desktop Linux

Recommended:

* separate executable;
* seccomp where practical;
* namespace isolation;
* private working directory;
* cgroup limits;
* loopback IPC.

macOS

Recommended:

* bundled helper executable;
* sandbox profile where feasible;
* hardened runtime;
* signed helper;
* fixed path;
* no shell.

Windows

Recommended:

* bundled signed executable;
* Job Object resource limits;
* restricted token;
* explicit handle inheritance;
* named pipe or loopback control.

Android

Helper subprocess support is more constrained.

Initial options:

* use Orbot/Tor through SOCKS;
* use an Android service packaging the upstream PT;
* later build narrowly reviewed in-process adapters.

iOS

General child-process execution is not a practical default.

Initial options:

* Network Extension-compatible native integration;
* external Tor-capable application where platform policy permits;
* defer native obfs4/WebTunnel until an auditable library boundary exists.

This platform difference is the strongest reason to retain both:

ManagedPtAdapter
NativePtAdapter

behind one Mininet trait.

⸻

17. Version selection

Mininet should not automatically track the newest upstream release.

For each adapter, define:

minimum supported version
current approved version
maximum tested version
security-denied versions

Upgrade process:

1. fetch exact upstream source;
2. review changelog and security issues;
3. reproduce build;
4. run upstream tests;
5. run Mininet adapter conformance tests;
6. run traffic compatibility tests;
7. run sandbox tests;
8. run downgrade tests;
9. record provenance;
10. ship through ordinary governed release.

⸻

18. Generic PT-manager scope

The first implementation should support only what Mininet needs.

Required

* launch client transport;
* request one named method;
* parse method readiness;
* obtain local endpoint;
* connect through local endpoint;
* monitor process;
* terminate cleanly;
* classify failures;
* collect safe version information.

Deferred

* arbitrary server-mode operation;
* extended ORPort support;
* every historical PT environment variable;
* unmanaged proxy chains;
* arbitrary plugins;
* user-supplied executable paths;
* descriptor-supplied arguments;
* hot reconfiguration of a running child;
* multiple unrelated transports in one child unless upstream requires it.

⸻

19. Bridge descriptor boundary

A descriptor may select:

pub enum ExternalTransport {
    LyrebirdObfs4,
    WebTunnel,
    Snowflake,
    Tor,
}

It may contain bounded transport parameters.

It must not contain:

* executable path;
* command line;
* environment;
* shell fragments;
* arbitrary file path;
* library name;
* network destination unrelated to the bridge;
* logging configuration.

The descriptor describes protocol data.

Local policy chooses implementation data.

⸻

20. Failure classification

The adapter should distinguish:

pub enum BridgeAdapterFailure {
    ExecutableUnavailable,
    ExecutableDigestMismatch,
    UnsupportedVersion,
    ProcessStartFailed,
    ProtocolNegotiationFailed,
    TransportUnavailable,
    BridgeRejected,
    BrokerUnavailable,
    NatTraversalFailed,
    Timeout,
    ProcessExited,
    SandboxViolation,
    InnerAuthenticationFailed,
    PolicyDowngradeDenied,
}

Do not report every low-level child error to remote peers.

Do not collapse all failures into "blocked."

Research evaluating several PTs found substantial performance and reliability variation, meaning transport failure does not by itself prove censorship. (arXiv)

⸻

21. Telemetry policy

Useful local metrics:

* adapter;
* approved version;
* startup success;
* connection stage;
* coarse latency;
* coarse network type;
* failure class;
* bridge descriptor age;
* process crash;
* resource use.

Forbidden telemetry:

* capability;
* DID;
* application object;
* destination user;
* message type;
* exact bridge secret;
* full Snowflake broker exchange;
* raw packet trace by default.

Shared telemetry should be opt-in, coarse, delayed, and threshold aggregated.

⸻

22. Integration test matrix

Common tests

1. Executable path is absolute and pinned.
2. Binary digest mismatch prevents execution.
3. Shell metacharacters are never interpreted.
4. Descriptor cannot change arguments.
5. Environment is allowlisted.
6. Child inherits no secret file descriptors.
7. Child cannot access identity storage.
8. Timeout terminates child.
9. Crash does not crash Mininet.
10. Restart does not reuse stale bridge secret accidentally.
11. Inner Mininet authentication is mandatory.
12. Direct fallback is denied when obfuscation is required.
13. Unsupported PT version fails closed.
14. Child logs redact secrets.
15. Resource exhaustion remains bounded.

Lyrebird tests

1. Upstream obfs4 client connects to upstream-compatible server.
2. Wrong bridge certificate fails.
3. Active probe without secret receives no Mininet handshake.
4. State directory isolation works.
5. Upstream version change triggers conformance tests.
6. Several parallel client sessions remain bounded.

WebTunnel tests

1. Real decoy website remains reachable where designed.
2. Correct HTTP Upgrade path connects.
3. Wrong path behaves like ordinary web failure.
4. TLS certificate validation follows deployment policy.
5. Reverse-proxy configuration does not expose Mininet headers.
6. Inner bridge authentication catches endpoint substitution.

Snowflake tests

1. Broker outage is classified separately.
2. No proxy allocation is classified separately.
3. ICE failure is classified separately.
4. Volunteer proxy cannot impersonate Mininet bridge.
5. UDP-restricted network produces a bounded failure.
6. Tor-managed mode and direct-managed mode remain separately labelled.
7. No claim of WebRTC indistinguishability appears in UI or docs.

Tor tests

1. SOCKS isolation credentials differ by privacy scope.
2. Onion destination is authenticated.
3. Tor process absence fails safely.
4. Control port is not needed for ordinary streams.
5. Control credentials never enter logs.
6. Mininet does not claim the selected Tor bridge itself provides mix-grade protection.

⸻

23. Decision matrix

Lyrebird

Criterion	Assessment
Upstream maturity	Strong
Operational simplicity	Moderate
Process isolation fit	Strong
Native-binding need	Low initially
Mobile difficulty	Moderate/high
Recommendation	Managed process

WebTunnel

Criterion	Assessment
Upstream maturity	Newer than Lyrebird
Operational simplicity	Moderate
HTTP/TLS deployment dependency	High
Process isolation fit	Strong
Native-binding need	Medium later
Recommendation	Managed process

Snowflake

Criterion	Assessment
Upstream maturity	Strong and extensive
Architecture complexity	High
External infrastructure dependency	High
Address agility	Strong
Process isolation fit	Strong on desktop
Native-binding need	Platform-dependent
Recommendation	Tor-managed first

Tor PT manager

Criterion	Assessment
Reuse across transports	Very high
Stable conceptual boundary	Strong
Security value	High
Implementation burden	Moderate
Recommendation	Build first

⸻

24. Recommended PR sequence

PR 1 — external adapter doctrine

Add:

docs/design/external-bridge-adapter-integration.md

Freeze:

* no shell;
* managed subprocess definition;
* binary provenance;
* trust boundary;
* sandbox;
* inner authentication;
* telemetry;
* platform constraints.

PR 2 — generic PT process manager

Implement:

* executable verification;
* minimal PT v1 client control;
* process lifecycle;
* loopback endpoint parsing;
* timeouts;
* structured failure;
* fake PT test executable.

No real PT dependency yet.

PR 3 — Lyrebird adapter

* approved version manifest;
* bundled or package-managed binary;
* obfs4 descriptors;
* connection tests;
* sandbox;
* integration harness.

PR 4 — WebTunnel adapter

* upstream process;
* descriptor schema;
* TLS/HTTP deployment guide;
* decoy behaviour tests;
* reverse-proxy test environment.

PR 5 — Tor compatibility bearer

* SOCKS5;
* stream isolation;
* onion endpoint;
* optional PT delegation to Tor;
* no broad control-port dependency.

This may precede WebTunnel depending on existing Tor support.

PR 6 — Snowflake via Tor

* select Snowflake transport through Tor;
* preserve Tor's broker/proxy ecosystem;
* classify failures;
* document limitations.

PR 7 — platform study

* Android helper-service prototype;
* iOS native feasibility;
* Arti evaluation;
* native-adapter criteria.

⸻

25. What Mininet should not build

Do not build a generic plugin execution system

Mininet needs a small approved PT set, not arbitrary executable plugins.

Do not allow user-provided PT binaries for high-assurance mode

Advanced users may experiment in developer builds, but production assurance cannot survive arbitrary binaries.

Do not move PT implementation into mini-crypto

Circumvention protocols are transport adapters, not cryptographic primitives.

Do not give PTs application plaintext

They carry an already encrypted and authenticated Mininet inner channel.

Do not use one adapter result as universal truth

A successful WebTunnel connection does not mean the network is uncensored.

A failed Snowflake allocation does not prove Snowflake is blocked.

⸻

26. Audit and review programme

Before marking an adapter production-ready, review:

Upstream

* current maintenance status;
* known vulnerabilities;
* protocol changes;
* release process;
* relevant public analyses;
* version-specific audit evidence where available.

Build

* source authenticity;
* reproducibility;
* dependency lock;
* toolchain;
* local patches;
* binary provenance.

Process boundary

* command construction;
* environment;
* IPC parsing;
* file descriptors;
* sandbox;
* resource limits;
* log handling;
* shutdown.

Protocol integration

* descriptor validation;
* bridge authentication;
* downgrade;
* replay;
* local proxy exposure;
* DNS handling;
* proxy isolation;
* timeout.

Operational fingerprinting

* traffic traces;
* TLS fingerprints;
* failure behaviour;
* active probing;
* unique Mininet configuration;
* server deployment templates.

⸻

27. Final technical recommendation

First choice

Build a Tor PT v1 managed-process adapter and integrate Lyrebird/obfs4 through it.

Second choice

Add WebTunnel using the same process boundary.

Third choice

Add Tor as an external SOCKS bearer, allowing Tor itself to own Snowflake initially.

Fourth choice

Expose Snowflake through Tor-managed selection before considering direct Snowflake operation.

Later

Evaluate:

* native Rust WebTunnel;
* native/mobile obfs4 library boundary;
* embedded Arti;
* direct Snowflake client;
* Mininet-operated Snowflake bridge infrastructure.

⸻

28. Proposed decision-log entry

Decision

Mininet integrates mature censorship-circumvention transports through a restricted managed-subprocess boundary implementing the Tor Pluggable Transport client protocol.

Lyrebird/obfs4 is the first adapter, followed by WebTunnel. Snowflake is initially consumed through Tor-managed transport selection rather than a Mininet-specific broker and proxy network.

External PT binaries are pinned by source and executable digest, built reproducibly where possible, sandboxed, and denied access to identity, capability, application, value, and governance state.

Every PT connection terminates into a separately authenticated Mininet bridge handshake.

Reason

Upstream PT implementations contain protocol behaviour and operational experience that Mininet should not recreate prematurely. A process boundary preserves updateability and memory isolation while one typed adapter contract prevents arbitrary plugin execution.

Constitutional impact

* strengthens entry censorship resistance;
* preserves transport choice;
* avoids one mandatory external network;
* prevents bridge implementations from becoming identity authorities;
* does not introduce novel cryptography;
* preserves honest protection labels;
* introduces no governance or economic authority.

Failure point

The design fails if untrusted descriptors can influence executable paths or arguments, if PT binaries access Mininet secrets, if upstream updates are accepted without provenance and testing, if the outer transport substitutes for Mininet bridge authentication, or if subprocess use becomes an unrestricted plugin framework.

Required follow-up

* PT process manager;
* Lyrebird integration;
* WebTunnel integration;
* Tor SOCKS isolation;
* Snowflake-through-Tor adapter;
* platform-specific sandboxing;
* version-specific external review;
* active-probing and fingerprint testing.

⸻

29. Final recommendations

Adopt

1. Tor PT v1 client process manager.
2. Exact executable allowlist.
3. No shell.
4. Absolute executable paths.
5. Source and binary digest pinning.
6. Reproducible build records.
7. Dedicated state directory.
8. Minimal environment.
9. Closed file descriptors.
10. Resource limits.
11. Process timeouts.
12. Structured failure classes.
13. Inner Mininet bridge authentication.
14. Lyrebird first.
15. WebTunnel second.
16. Tor SOCKS bearer.
17. Snowflake through Tor first.
18. Platform-specific adapter strategy.
19. Packet-trace compatibility tests.
20. Honest audit terminology.

Defer

1. Direct Snowflake ecosystem operation.
2. Native Rust obfs4.
3. Native Rust WebTunnel.
4. In-process Go bindings.
5. Embedded Arti.
6. Server-side generic PT hosting framework.
7. User-installable PT marketplace.
8. Automatic runtime binary downloads.
9. Provider-specific domain fronting.
10. Custom traffic mimicry.

Reject

1. sh -c.
2. Executable paths from bridge descriptors.
3. Arguments from untrusted descriptors.
4. PATH-based executable discovery.
5. Arbitrary PT plugins in production.
6. PT access to root keys.
7. PT access to capability stores.
8. PT access to application plaintext.
9. Rewriting obfs4 without need.
10. Forking Snowflake infrastructure prematurely.
11. Claiming upstream deployment equals a complete independent audit.
12. Calling Snowflake indistinguishable WebRTC.
13. Calling WebTunnel ordinary web traffic without measurement.
14. Silent direct fallback.
15. Treating process isolation as sufficient without bridge authentication.

⸻

30. Essay: The Safest Code May Be the Code Kept Outside

Software projects often treat native integration as maturity.

A subprocess is considered temporary. A library binding appears cleaner. A rewrite in the project's preferred language appears cleaner still.

For censorship-circumvention transports, that instinct can be dangerous.

A pluggable transport is not only a serializer wrapped around a socket. Its security lives in details that may not appear in a concise protocol description:

* exactly how an invalid client is rejected;
* whether the first packet has a recognisable size;
* how replay state is managed;
* which TLS extensions appear;
* how an HTTP server responds to probes;
* whether connection timing differs after authentication;
* how NAT traversal behaves;
* what happens when the broker fails;
* how implementation errors appear on the wire.

A clean-room rewrite may reproduce the documented wire format and still acquire a new fingerprint.

The rewrite may even be memory safe and easier to read while becoming easier to block.

This is why Mininet should initially prefer a process boundary around established implementations.

The process is not trusted because it is upstream.

It is isolated because it is network-facing, complicated, and independently evolving.

Mininet should know exactly which executable it launches, exactly which source produced it, exactly which arguments it receives, and exactly which files and network destinations it may access. It should know when the process started, what transport it offered, and why it failed.

It should not give the process an identity key.

The process transforms the outside of a connection. Inside that transformed connection, Mininet performs its own authenticated handshake. This preserves the line between camouflage and authority.

An obfs4 process may make the traffic harder to classify or probe. It does not decide which Mininet bridge is genuine.

A WebTunnel process may carry bytes through an HTTPS-compatible path. It does not grant a mailbox capability.

A Snowflake proxy may provide a rapidly changing route around address blocks. It does not learn the application plaintext and cannot become the user's identity.

This separation also protects Mininet from one of the most common supply-chain mistakes: mistaking installation for delegation of architectural control.

Using an upstream binary does not mean accepting every future release automatically. Mininet must pin the source, reproduce the build, test the binary, record its digest, and carry it through the same governed release process as its Rust crates.

The executable is external in process, not external to responsibility.

At the same time, Mininet should resist turning the PT manager into a general plugin platform.

A platform that executes arbitrary user-provided commands is flexible. It is also impossible to describe honestly as a reviewed security boundary. Production should support a small list of approved adapters whose binaries, protocols, and resource requirements are known.

Lyrebird should come first because it exercises the architecture with a relatively contained transport. WebTunnel follows because it introduces public web-facing deployment and camouflage behaviour. Snowflake follows through Tor because its strength comes not only from client code but from an existing broker and volunteer-proxy ecosystem.

Reimplementing the Snowflake client while operating no mature proxy population would reproduce complexity while discarding the network effect that gives the system value.

Eventually, mobile platforms may force some transports in-process. iOS does not offer the same helper-process model as desktop Linux. Android service packaging has its own restrictions. Those constraints justify native adapters later.

They do not justify making the first implementation carry the risk of every future platform.

The right progression is:

stable process boundary
→ operational experience
→ trace and compatibility tests
→ specific mobile need
→ narrow reviewed native adapter

not:

rewrite everything in Rust
→ discover protocol fingerprints in production

The deepest purpose of a pluggable transport is replaceability.

A censor learns. A transport that works today may fail next year. An implementation may be retired. A broker may become unreachable. A web camouflage technique may lose its collateral protection.

Mininet should therefore make the adapter contract permanent and each transport replaceable.

The process boundary is not an embarrassing temporary layer.

It is one of the mechanisms that keeps the trusted core smaller than the changing censorship battlefield around it.

The strongest first PR is the generic managed PT process adapter with a fake conformance child, followed by a separate Lyrebird integration PR. That proves the safety boundary before a real circumvention binary becomes part of the release.
