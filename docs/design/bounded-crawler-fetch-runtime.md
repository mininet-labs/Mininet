# Bounded crawler fetch runtime

**Decision:** proposed D-0425
**Maturity:** tested implementation proposal; not deployed or externally audited

## Purpose

`mini-crawler` deliberately stops before network I/O. This slice adds the
smallest real execution boundary that can turn one already-admitted
`CrawlRequest` into fetched bytes and a `CrawlObservation` without granting an
arbitrary crawl job access to the participant's local network.

It implements the HTTP/HTTPS, strict-limit, no-JavaScript, fetch-receipt and
content-digest portion of MiniSearch Track E3. A `CrawlObservation` is an
observation record, not proof that the page was globally available, truthful,
useful, independently corroborated, or reward-eligible.

## Trust boundary

```text
deterministic CrawlPlan
        |
        v
explicit robots decision ---- Unknown fails closed
        |
        v
resolve host + reject mixed/non-public answers
        |
        v
pin approved addresses into no-redirect HTTP client
        |
        v
bounded response ---- redirect? repeat the entire boundary
        |
        v
supported bytes + canonical observation identity
```

The `FetchBackend` contract requires an implementation to connect only to the
addresses supplied by the runtime. `ReqwestBackend` satisfies that contract via
`ClientBuilder::resolve_to_addrs`. Redirects are disabled in reqwest and
implemented above it, because automatic redirect handling could cross the DNS
and address-policy boundary without reauthorization.

Mixed DNS answers fail closed. This sacrifices some availability when a host
returns both public and prohibited addresses, but avoids letting backend
address selection turn a validated public answer into a private connection.

## Default policy

- HTTPS only;
- standard ports only;
- at most five redirects;
- 5-second connection and 20-second request timeout;
- at most 8 MiB of response body;
- `Accept-Encoding: identity`, with no transparent gzip/Brotli/deflate/zstd;
- explicit robots `Allowed` required;
- HTML, plain text, Markdown, JSON, PDF and images admitted;
- credentials and non-HTTP(S) redirect destinations rejected;
- loopback, private, link-local, multicast, documentation, benchmarking,
  carrier-grade-NAT, unspecified and IPv4-mapped IPv6 destinations rejected.

The caller can deliberately enable HTTP or nonstandard ports through
`FetchLimits`; the default remains conservative. There is no switch to enable
private destinations. A future intranet crawler must be a separately typed
mode with its own authority and UI, not a boolean accidentally reused by
untrusted public crawl jobs.

## Observation identity

`derive_observation_id` hashes a versioned, domain-separated canonical encoding
of every public observation field except the id itself. Status and media types
use explicit numeric tags; Rust debug output and memory representation are not
part of the digest. The fetched body's BLAKE3 multihash and exact byte length
bind successful observations to the returned bytes.

## Rejected alternatives

- **Automatic redirects:** rejected because every hop requires new URL, DNS,
  address and port authorization.
- **Resolve, validate, then let the HTTP client resolve again:** rejected as a
  DNS-rebinding/time-of-check-to-time-of-use vulnerability.
- **Transparent decompression:** rejected until compressed and expanded byte
  budgets are independently represented and tested.
- **Private-address test override:** rejected because production callers could
  accidentally expose it. Tests use an injected backend instead.
- **Embedding the runtime in `mini-crawler`:** rejected to preserve the small,
  deterministic, dependency-light planning core.

## Failure and recovery

Every ordinary fetch failure becomes an explicit `FetchStatus` observation.
Unknown robots policy is a caller error and schedules no network work. Partial
bodies are discarded. The runtime has no canonical state and can be retried by
a scheduler; deduplication, retry timing and persistence remain outside this
crate.

## Explicitly unfinished

- robots.txt retrieval, parsing, expiry and cache;
- per-origin politeness and concurrency scheduling;
- durable frontier leases, retry state and crash recovery;
- proxy/bridge/mix transport and censorship-resistance drills;
- isolated HTML/PDF/image parsing and malware scanning;
- compressed responses under separate wire/expanded limits;
- HTTP caching, conditional requests and sitemap discovery;
- end-to-end storage, extraction, index construction and publication;
- independent corroboration or TLS transcript commitments;
- crawler rewards, anti-collusion evidence and settlement;
- load/soak testing and external security review.

Production crawler operation is blocked on these layers even though bounded
single-request execution is implemented and tested here.
