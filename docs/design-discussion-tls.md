# Design discussion — TLS and the net surface (rustils#70)

Not a decision record. This is the RFC-level research the owner asked for on
rustils#70's question — "when `rusty_request` (via `rusty_tokio`) eventually
needs HTTPS, what is the intended seam, and does anything belong in rustils?" —
written *before* any consumer passes §3's gate, on the same posture as
`docs/design-discussion-sandbox.md`: surface what the consumers, the operating
systems, and the ecosystem's own precedents actually offer, and the real open
questions that follow, for the owner to decide. Per §3, this document produces
rows and gate conditions, not code.

One honesty note up front, since the sandbox discussion set the standard of
verifying donors against their source: the donor repos (shh, rusty_rdp,
rusty_tail, rusty_llama) are not checked out in this workspace. Claims about
them below are grounded in this repo's own records (the extraction map's D16
survey, `docs/behavior/net.md`, `docs/behavior/security.md`'s Csprng history)
and in rustils#70's own description of `rusty_request` — not fresh source
verification. Anything that turns on a donor detail this repo hasn't recorded
is flagged as such.

## Context: the standing stance, restated precisely

The stance is not "rustils has no opinion on TLS." It is three specific,
already-recorded decisions:

1. **D16 (extraction map): no TLS obligation in `Net`.** All four named net
   consumers bring or inject their own wire crypto; `Net`/`TcpStream` carry no
   TLS concept, and `net.md` lists TLS/crypto under *Deliberately unspecified*
   — a designed exclusion, not a gap.
2. **The injection precedent (rusty_rdp).** rdp's `tls.rs` layers optional,
   injected rustls over code generic in `Read + Write` — TLS wraps the plain
   stream *above* the trait boundary.
3. **The narrow-extraction precedent (Csprng).** The one crypto-adjacent thing
   that *did* land here came from that same `tls.rs`: five hand-rolled
   `/dev/urandom` reads retired by `Csprng::fill_random` — one method, no key
   derivation, no algorithm choice, because that was all the named consumer
   needed.

So the research question is not "should rustils do TLS" in the abstract — it is
*which parts of the HTTPS-client problem, if any, are Layer-1/Layer-2 material
under this architecture, and which are permanently Layer-3*.

## What an HTTPS client actually needs

The minimum a real `https://` client must do, annotated by what kind of
problem each row is. This decomposition is the whole document in miniature —
every option below is a claim about where these rows live.

| # | Need | Kind of problem |
|---|---|---|
| 1 | TLS handshake + record protocol (1.2/1.3: key schedule, X25519/P-256 key exchange, AEAD record encryption, finished-message verification) | Protocol cryptography |
| 2 | X.509 chain building and validation (ASN.1/DER parsing, signature verification up the chain, expiry, name constraints, EKU) | Protocol cryptography + PKI logic |
| 3 | **Trust anchors** — the set of root certificates the chain must terminate in | **OS personality** — every OS keeps these somewhere different, in a different shape |
| 4 | Hostname verification (SAN/dNSName matching, wildcard rules) | Protocol logic (RFC 6125) |
| 5 | SNI (required by effectively every CDN-hosted server) | Protocol logic |
| 6 | ALPN (only if HTTP/2 ever; `http/1.1`-only clients can skip it) | Protocol logic |
| 7 | Session resumption / tickets | Protocol logic, optional (performance only) |
| 8 | Revocation (OCSP/CRL) | PKI logic, realistically optional for a non-browser client |
| 9 | A plain, nonblocking-capable byte stream underneath | **Already solved** — D16's boundary |

Rows 1–2 and 4–8 are the TLS library. Row 9 is done. Row 3 is the one row that
is *genuinely* about the operating system's personality — and notably, it was
never actually excluded by D16, which excluded wire-crypto behavior from the
*trait*, not trust-material access from the *repo*.

## What each OS actually offers (the Layer 0/1 survey)

This is the load-bearing section, because the three platforms are radically
asymmetric, and every design option inherits that asymmetry.

### Windows — TLS is a first-class OS service

- **A full TLS engine: SChannel via SSPI.** `AcquireCredentialsHandle` /
  `InitializeSecurityContext` / `EncryptMessage` / `DecryptMessage`. Notably
  for this repo's tastes, the SSPI handshake is *sans-IO*: the caller pumps
  opaque token buffers between the API and its own socket, so it layers over
  a nonblocking `TcpStream` (or `rusty_tokio`'s) without owning the I/O. All
  reachable from the existing `windows-sys` floor.
- **A full chain engine: CryptoAPI.** `CertGetCertificateChain` +
  `CertVerifyCertificateChainPolicy(CERT_CHAIN_POLICY_SSL, ...)` performs
  complete validation — chain building against the ROOT store, revocation if
  asked, hostname matching — as one OS call.
- **Trust anchors as an OS object.** `CertOpenSystemStoreW(L"ROOT")` +
  `CertEnumCertificatesInStore` yields the roots as DER blobs, honoring
  enterprise group-policy-deployed roots — something a bundled cert file can
  never do.

### macOS — half a TLS service, a whole trust service

- **Trust evaluation is first-class and current:** `SecTrustCreateWithCertificates`
  + `SecTrustEvaluateWithError` with an SSL policy (`SecPolicyCreateSSL`)
  validates a chain including hostname, against the keychain's trust settings
  (again including MDM/enterprise-deployed roots). Anchor enumeration exists
  (`SecTrustCopyAnchorCertificates`), though Apple steers callers toward
  evaluation rather than enumeration.
- **The TLS engine story is worse:** Secure Transport (custom-IO callbacks, so
  it *can* layer over an external stream) is deprecated since 10.15; its
  replacement, Network.framework, owns the connection down to the socket —
  it does not layer over a stream someone else owns, which is exactly the
  wrong shape for wrapping `rusty_tokio::io::TcpStream`.
- Repo-local caveat: `platform-macos` is deliberately net-only and thin.
  Anything here means linking Security.framework — a whole new framework
  admission for that backend, not a marginal addition.

### Linux — no OS TLS engine, no OS verifier, anchors are just files

- **The kernel's only TLS concept is kTLS** (`setsockopt(TCP_ULP, "tls")` +
  `SOL_TLS` key material): record-layer encrypt/decrypt offload *after* a
  userspace handshake has already derived the keys. It is a throughput/
  `sendfile(2)` feature, not a TLS implementation — there is nothing to
  hand a handshake to.
- **There is no OS chain verifier.** No syscall, no stable system service.
  OpenSSL/GnuTLS are distro *libraries*, not kernel ABI — building on them is
  the libc-tier question Track P exists to escape, but worse (soname churn,
  distro feature drift), and it would be a hard dependency where libc is at
  least universal.
- **Trust anchors are plain files with no single home:**
  `/etc/ssl/certs/ca-certificates.crt` (Debian),
  `/etc/pki/tls/certs/ca-bundle.crt` (RHEL/Fedora),
  `/etc/ssl/cert.pem` (Alpine), plus the `SSL_CERT_FILE`/`SSL_CERT_DIR`
  conventions. Reading them is `Fs` work; *knowing where to look* is the
  entire per-distro personality.

**The central finding:** "let the OS do it" is fully true on Windows, true
only for trust evaluation on macOS, and false on Linux — the platform
`rusty_tokio`/`rusty_request` actually develop on. Any design that leans on
an OS TLS engine is `Unsupported` exactly where the named consumer lives.

The wider Rust ecosystem independently converged on this same reading, which
is worth taking as confirmation rather than as a dependency plan (learn from,
don't depend — the §5.3 cap-std posture): `rustls-native-certs` exists solely
to do row 3 per-OS (schannel ROOT store / Security.framework / file-path
probing on Linux), and `rustls-platform-verifier` exists solely to do row 2
via the OS on Windows/macOS *with a webpki fallback on Linux* — because on
Linux there is nothing to delegate to.

## Prior art inside the ecosystem

- **rusty_rdp** (per the extraction map): the injection seam works in
  production — rustls optional, injected, over `Read + Write`-generic code —
  and the Csprng extraction proves the "retire the OS-facing duplication into
  one narrow primitive" motion works from exactly this neighborhood.
- **shh and rusty_tail hand-roll their wire crypto — and that does *not*
  argue TLS is similarly hand-rollable.** SSH-style and Noise-style protocols
  have no X.509/PKI, no ASN.1, no version-and-ciphersuite negotiation matrix,
  and (in those tools' deployments) pinned or TOFU keys. TLS + WebPKI is a
  different order of attack surface, and its characteristic failures are
  *silent* — a validator that accepts a bad chain passes every happy-path
  test ever written for it. The parity regime has no oracle for "rejects what
  an attacker sends"; contrast Track P, where a wrong syscall wrapper fails
  loudly under the existing suite. M1's own rule — hand-rolled only counts
  when it's *correct* — is unusually hard to satisfy here and unusually
  expensive to check.
- **rusty_llama's optional server** is recorded in D16 as one of the four
  bring-your-own-crypto consumers; nothing in this repo's records suggests it
  has a TLS shape beyond the same injection pattern. (Flag: if its actual
  source shows otherwise, that's new information this document didn't have.)

## The options

### Option A — status quo: the consumer injects a TLS layer (issue #70's option 1)

`rusty_request` (or `rusty_tokio` as a middleware layer) wraps the plain
stream in an async TLS implementation built or vendored outside this repo,
exactly as rdp does synchronously.

**Seam check — is the boundary already sufficient?** What a sans-IO TLS layer
needs from the stream underneath: read/write with honest `WouldBlock`
(`rusty_tokio`'s reactor path — raw-fd/raw-socket access + `set_nonblocking`,
landed as rustils#41/#48/#59 for Linux/macOS/Windows precisely for this
consumer), or blocking read with `set_read_timeout` (landed 0.13-era, forced
by rdp). Nothing else — TLS needs no socket options `Net` doesn't already
expose. **The seam is complete today; Option A costs rustils zero code.**

The one real cost lands on `rusty_request`: taking rustls (or vendoring an
async TLS layer) cuts against its hand-rolled ethos. That is a Layer-3 values
question this repo's architecture deliberately leaves to the consumer — same
as it left shh's and rusty_tail's crypto choices to them.

### Option B — a narrow trust-material primitive (issue #70's option 2, split honestly)

The issue's "certificate validation / trust-store access via the OS" is
**two different primitives wearing one label**, and the platform survey above
splits them cleanly:

- **B1 — trust-anchor access: "give me the OS's root certificates," as raw
  DER blobs.** Implementable *symmetrically* on all three backends
  (CertEnumCertificatesInStore / SecTrustCopyAnchorCertificates / distro-path
  probing + `SSL_CERT_FILE` on Linux). No cryptography enters rustils — the
  consumer's TLS layer does all validation; rustils only answers the genuinely
  OS-personality question of *where the roots live and how to read them*.
  Naturally one method; naturally mockable (a mock backend with configurable
  anchors gives TLS-using consumers hermetic tests, the same service
  `platform-mock` provides everywhere else); honors enterprise-deployed roots
  on Windows/macOS, which no bundled-certs approach can. **This is the
  Csprng-shaped candidate.**
- **B2 — OS chain *verification*: "validate this presented chain for this
  hostname."** A real one-call OS API on Windows and macOS; **no OS API at
  all on Linux.** A Linux backend would have to hand-roll X.509 path
  validation — ASN.1 parsing plus RSA/ECDSA signature verification — which is
  precisely the crypto this repo refuses, smuggled in through a "narrow"
  door. So B2 is honest only as an *asymmetric, platform-optional* surface
  (`Unsupported` on Linux — mechanically precedented by Tun-on-Windows and
  Sandbox-on-Linux, but those served consumers who lived on the supported
  platform; here the named consumer develops on the unsupported one). B2
  should never be the primary path, only a possible later upgrade for a
  consumer that specifically wants OS-policy semantics on Windows/macOS —
  the exact posture rustls-platform-verifier takes.

Gate condition to record for B1 (drafted here so whoever files it isn't
guessing): *`rusty_request` (or any consumer) ships working HTTPS with an
injected TLS layer, and is observed hand-rolling OS-anchor loading — distro
path probing, cert-store reading — at real call sites.* That is the same
duplication signal that gated Csprng, and it cannot exist before Option A
happens first.

### Option C — an OS-native TLS engine surface (`SChannel`/Secure Transport behind a trait)

Genuinely M1-attractive on Windows — the SSPI token pump is a first-rate NT
lesson, and its sans-IO shape fits this repo's boundary instincts. But:

- **No Linux backend can exist** without pulling in a TLS library, so the
  named consumer's own platform is permanently `Unsupported` — the surface
  would serve a consumer that hasn't been named instead of the one that has.
- Secure Transport is deprecated; its successor can't wrap an external stream.
- The surface is irreducibly wide (protocol versions, ALPN, resumption,
  alert/error taxonomy, renegotiation) — the opposite of the one-method
  discipline every admitted primitive here has followed.
- The parity regime's textually-identical-suites convention has no good story
  for a domain where one backend is structurally absent rather than divergent
  on lines.

**Recorded as researched and not recommended.** Revisit only if a
Windows-only (or Windows-first) consumer names itself with a concrete need —
at which point the SSPI half stands on its own merits.

### Option D — hand-rolled TLS inside rustils

Concrete scope, so the size is on record: a TLS 1.3 client alone means the
handshake state machine, the key schedule (HKDF), X25519, at least one AEAD
(AES-GCM or ChaCha20-Poly1305), RSA-PSS + ECDSA signature verification,
X.509/ASN.1 DER parsing, SAN/hostname matching, and the record + alert
layers — rows 1, 2, 4, 5 of the needs table, every one security-critical,
most failing silently when wrong (see the shh/rusty_tail contrast above).
There is no parity oracle for "resists an attacker," and no OS ground truth
to diverge-registry against. This is the one option both M1 and M2 argue
*against* rather than trading off: M1 because correctness is unverifiable at
this repo's tooling level, M2 because a foundation must not carry a
hand-rolled validator other projects then trust.

**Not recommended in rustils, at any layer.** If the owner ever wants the
learning (it is real — M1 doesn't stop being curious because the artifact is
dangerous), the shape that fits the architecture is a Layer-3 toy *beside*
the PAL — a `rusty_tls` in the rusty_lines/rusty_regx position — explicitly
never wired into anything that trusts its verdicts.

### Option E — kTLS, for completeness

Linux-only record-layer offload after a userspace handshake; useful when a
consumer wants `sendfile(2)`-over-TLS throughput (static file serving), i.e.
no consumer in this ecosystem today and plausibly ever. Pure kernel-ABI
material, so it *would* be Layer-1-shaped if a consumer appeared. Row only;
nothing to design.

## The core tension: "TLS support" is three problems wearing one label

Mirroring the sandbox discussion's finding that "sandbox" named two unlike
things:

1. **The protocol/crypto engine** (needs 1–2, 4–8) — OS personality on
   Windows, deprecated-or-wrong-shaped on macOS, nonexistent on Linux.
   Permanently Layer 3 under this architecture (Options A/D).
2. **Trust-material access** (need 3) — OS personality on *every* platform,
   just asymmetrically shaped (store API vs. framework call vs. file
   probing). The only Layer-2 candidate, and D16 never actually excluded it
   (Option B1).
3. **The wire seam** (need 9) — already built, already proven by the
   #41/#48/#59 sequence that this exact consumer forced.

The standing stance "no TLS in rustils" is precisely "no (1) in rustils."
Keeping that stance and eventually landing B1 are fully compatible.

## Recommendation (research input, not the decision)

- **Now:** Option A, which requires nothing — the seam is verified complete
  above. `rusty_request` stays `http://`-only or injects TLS at Layer 3 when
  ready; rustils changes nothing.
- **Pre-identified gated slice:** B1 (`load OS trust anchors as DER blobs`),
  fourth Security slice alongside Csprng/Sandbox, unparked only on the gate
  condition drafted above. Raw DER at the boundary — no ASN.1 types in
  rustils, the byte-oriented §5.2 instinct applied to certificates; parsing
  stays consumer-side.
- **Recorded as researched-and-declined:** C (no backend on the consumer's
  platform), D (unverifiable correctness in a trust-critical artifact), E
  (no consumer, trivially revisitable).
- **B2** noted as a possible much-later Windows/macOS-only upgrade behind
  the same primitive's door, never the primary path.

## Open questions for the owner, not decided here

1. **Does B1 wait for its gate, or is a Sandbox-style speculative build
   acceptable?** The Sandbox precedent shows the owner may explicitly accept
   speculative-build risk when a validated donor design exists. B1 has no
   equivalent donor implementation in the ecosystem (rdp injects rustls but
   this repo has no record of it loading *native* anchors) — which argues for
   waiting, per the CredentialStore outcome rather than the Sandbox one.
2. **Which domain does B1 land in if/when gated — `security` (beside Csprng,
   as trust *material*) or `net`?** Recommendation embedded above: `security`,
   which also keeps `net.md`'s "Deliberately unspecified — any TLS/crypto"
   line true forever.
3. **Linux anchor-source policy for B1:** probe the known distro paths only,
   honor `SSL_CERT_FILE`/`SSL_CERT_DIR` first, or both — and what does
   `platform-mock` assert about ordering/precedence? (This is where all the
   real Linux design content lives; Windows/macOS are single API calls.)
4. **Who is the consumer for gating purposes — `rusty_request` or
   `rusty_tokio`?** The TLS layer could live in either (request-level vs. a
   runtime-provided stream wrapper). It changes nothing about rustils' seam,
   but §3's table wants a *name*, and the anchor-loading call sites will live
   wherever the TLS layer does.
5. **Is fresh donor verification needed before any of this advances?** This
   document could not check shh/rdp/llama source directly (noted up top). The
   sandbox discussion's standard says: verify against source, not docs'
   framing, before the RFC-level call — in particular whether rusty_llama's
   server TLS shape matches D16's record, and whether rdp's rustls path loads
   native anchors today (which would make rdp, not rusty_request, B1's first
   real call site).

## What this document does not decide

Whether B1 ever gets built, on what timeline, or in what exact shape — and
nothing here weakens §3: `rusty_request` has no HTTPS code path today, so no
gate is passed and no code follows from this document. This is the input to
the owner's call that rustils#70 asked to have on record, not the call
itself.
