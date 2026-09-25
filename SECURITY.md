# Security Policy

## Scope

RECMeetily is a local-only macOS desktop application (Tauri 2 + Rust). It
records, transcribes and summarizes meetings entirely on-device. There is no
bundled server, no telemetry, and no automatic network access:

- The frontend is a static Next.js export (`next export`), loaded by Tauri's
  WebView directly from disk. There is no Next.js server process at runtime,
  in development or in the shipped app.
- The only outbound network calls the app makes are (a) hash-verified model
  and binary downloads the user explicitly triggers, and (b) requests to a
  cloud LLM provider (OpenAI, Anthropic, Groq, OpenRouter, Ollama, or another
  OpenAI-compatible endpoint) if and only if the user configures an API key
  for one.
- No inbound network exposure: the app does not listen on any port that
  accepts connections from other machines.

## Reporting a Vulnerability

Please open a GitHub issue on
[felipecortesp/RECMeetily](https://github.com/felipecortesp/RECMeetily). For
anything you'd rather not post publicly first, mention that in the issue
title and we'll follow up privately before any public discussion.

## Dependency Policy

Before each release:

- `cargo audit` and `cargo deny check` (advisories, bans, licenses, sources)
  against the Rust workspace (`frontend/src-tauri`, `llama-helper`).
- `pnpm audit --prod` against the frontend (`frontend/`).

Findings that can be fixed with a semver-compatible `cargo update` /
`pnpm update` are fixed immediately. Findings that require a major-version
upgrade are tracked as a follow-up task and, if they cannot affect this app
given how it actually runs, documented below with the reason.

## Accepted / Not Applicable

### Frontend (pnpm audit)

Next.js is pinned to the 14.2.x line (currently 14.2.35). All of its
outstanding advisories below are fixed only in Next.js 15.x, which is a
major-version upgrade out of scope for this pass. They do not apply to
RECMeetily's runtime because **the app never runs a Next.js server**: it
loads a static export from disk inside Tauri's WebView. Server-side request
handling, middleware, Server Actions, the Image Optimization API, and React
Server Components are all Next.js *server* features that never execute.

| Advisory | Package | Reason not applicable |
|---|---|---|
| GHSA-p293-qw3h-jr36 | next | RCE in a Windows-hosted Next.js server; no server runs |
| GHSA-2xp9-vwfh-vxw4 | next | RCE in the Image Optimization API server route; not served |
| GHSA-h25m-26qc-wcjf | next | DoS via server-side RSC request deserialization; no server |
| GHSA-q4gf-8mx6-v5v3 | next | DoS with Server Components (server-side); no server |
| GHSA-8h8q-6873-q5fj | next | DoS with Server Components (server-side); no server |
| GHSA-c4j6-fc7j-m34r | next | SSRF via WebSocket upgrade handling; no server |
| GHSA-36qx-fr4f-26g5 | next | Middleware/Proxy bypass (Pages Router i18n); no middleware runs |
| GHSA-m99w-x7hq-7vfj | next | DoS in Server Actions; no server |
| GHSA-89xv-2m56-2m9x | next | SSRF in Server Actions on custom servers; no server |
| GHSA-p9j2-gv94-2wf4 | next | SSRF in rewrites; rewrites are a server/middleware feature, not evaluated in a static export |
| GHSA-9g9p-9gw9-jx7f | next | DoS via Image Optimizer remotePatterns; Image Optimization server route not served |
| GHSA-ggv3-7p47-pfv8 | next | HTTP request smuggling in rewrites; no server |
| GHSA-3x4c-7xq6-9pq8 | next | Unbounded next/image disk cache growth; Image Optimization server route not served |
| GHSA-ffhc-5mcf-pf4q | next | XSS via CSP nonces in App Router; server-rendering feature, static export has no per-request nonce |
| GHSA-gx5p-jg67-6x7h | next | XSS in beforeInteractive scripts; only exploitable with untrusted server-supplied input, we don't inject any |
| GHSA-h64f-5h5j-jqjh | next | DoS in Image Optimization API; not served |
| GHSA-wfc6-r584-vfw7 | next | Cache poisoning in RSC responses; no server-side cache |
| GHSA-68g3-v927-f742 | next | Cache confusion for requests with bodies; no server |
| GHSA-4633-3j49-mh5q | next | Cache confusion (invalid UTF-8 bodies); no server |
| GHSA-4c39-4ccg-62r3 | next | Unbounded Server Action payload (Edge runtime); no server/edge runtime |
| GHSA-955p-x3mx-jcvp | next | Disclosure of internal Server Function endpoints; no server |
| GHSA-3g8h-86w9-wvmq | next | Middleware/Proxy redirect cache poisoning; no middleware/server cache |
| GHSA-vfv6-92ff-j949 | next | Cache poisoning via RSC cache-busting collisions; no server-side cache |

### Frontend - accepted, tracked as follow-up (not "not applicable")

These two run inside the app (in the WebView, client-side) so the "no
server" argument does not excuse them. They are blocked by `@blocknote/core`
pinning an old `uuid` and `@tiptap/core` 2.x internally; fixing them needs a
`@blocknote/*` major-version upgrade (0.36.0 -> 0.55.0, itself a large,
breaking jump), which is out of scope for this dependency-audit pass.

| Advisory | Package | Notes |
|---|---|---|
| GHSA-w5hq-g745-h8pq | uuid (`<11.1.1`, via `@blocknote/core`) | Buffer bounds check only affects v3/v5/v6 when a caller-supplied `buf` is passed; BlockNote generates ids without `buf` as far as we've checked, but this has not been independently verified against BlockNote's source. Tracked for the `@blocknote` upgrade task. |
| GHSA-cp6q-959q-f8rh | @tiptap/core (`<3.30.4`, via `@blocknote/core`) | `mergeAttributes()` `__proto__` prototype pollution; fix needs Tiptap 3.x, which BlockNote 0.36.0 does not use. Tracked for the `@blocknote` upgrade task. |

### Rust workspace (cargo audit / cargo deny)

See `deny.toml` (`[advisories.ignore]`) and `.cargo/audit.toml` for the full,
per-advisory reasoning kept in sync with this table.

| Advisory | Crate | Reason |
|---|---|---|
| RUSTSEC-2023-0071 | rsa 0.9.10 | Marvin Attack timing sidechannel, no upstream fix. Pulled only via sqlx's unused "mysql" feature; never compiled on macOS. |
| RUSTSEC-2026-0258 | h2 0.3.27 | Unbounded empty DATA frames. Fix needs h2 >=0.4.16 via reqwest 0.12 (major bump), deferred. |
| RUSTSEC-2026-0293 | ringbuf 0.4.8 | Double free / use-after-free when a `Drop` panics. Fix needs ringbuf 0.5.x, a breaking API change from our pinned 0.4.8; deferred to a scoped follow-up. |
| RUSTSEC-2024-0375 / RUSTSEC-2021-0145 | atty 0.2.14 | Unmaintained + unsound (unaligned read). Pulled by nnnoiseless 0.5.2 (latest release) -> clap 3.2.25; no fix upstream. |
| RUSTSEC-2024-0370 | proc-macro-error 1.0.4 | Unmaintained. Only reachable via the Linux GTK tray-icon backend; never compiled on macOS. |
| RUSTSEC-2025-0134 | rustls-pemfile 1.0.4 | Unmaintained. reqwest 0.11 pins the 1.x line; maintained 2.x needs the deferred reqwest 0.12 bump. |
| RUSTSEC-2025-0081/0075/0080/0100/0098 | unic-char-property / unic-char-range / unic-common / unic-ucd-ident / unic-ucd-version 0.9.0 | Unmaintained, no release since 2020. Transitive dependency of tauri-utils via urlpattern, not under our control. |
| RUSTSEC-2024-0429 | glib 0.18.5 | Unsound `VariantStrIter` iterator impls. Fix needs glib >=0.20; only reachable via the Linux GTK backend, never compiled on macOS. |
