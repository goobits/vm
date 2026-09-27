# Code-scanning review — 2026-09-27

Scope: 33 Rust CodeQL alerts reported on main revision
`097be68bb2d68a024794db35e2c991ba41a37c9c`, reviewed against
`1834c6f72a6c67536f969168bcf9c2c5e7446f26` and the fixes in
[PR #102](https://github.com/goobits/vm/pull/102).

An empty PR analysis is not evidence that main has no alerts. Main scanning
populated these findings after the earlier PR checks completed. Workflow badges
show check status, not a guarantee of vulnerability-free code.

## Genuine findings and remediation

- **#1, PyPI HTML injection:** authenticated publishers could upload a filename
  containing markup which the simple index interpolated into link text and hrefs.
  The renderer now escapes dynamic HTML, encodes URL path segments and hash
  fragments, and uses normalized package titles. Local and upstream HTML responses
  have a restrictive CSP (`default-src 'none'; base-uri 'none'; form-action
  'none'; sandbox`). Upstream package metadata and link destinations remain
  trusted; it has no script or same-origin authority in supporting browsers.
  Tests cover an in-memory markup attack and a portable upload/index/download
  roundtrip with metacharacters, Unicode, and exact downloaded bytes.
- **#12–14, auth transport:** current CLI calls used literal loopback, but the
  public client accepted arbitrary plaintext remote URLs and default proxy/redirect
  behavior. All auth requests now require HTTPS except for literal loopback IPs,
  ignore environment proxies, and reject redirects. The built-in HTTP server
  rejects non-loopback binds before creating secret storage. Remote use requires
  TLS termination. HTTP error bodies are excluded from errors because they could
  echo credentials. Tests cover URL boundary cases, live redirect refusal,
  non-disclosure of server error bodies, and rejected listener initialization.

These source fixes require a new main-branch analysis before declaring the
reported alerts resolved. Do not dismiss a real defect solely to obtain zero alerts.

## Logging findings

- **#10, Tart stdin, false positive:** `TartProvider::exec_with_stdin` passes
  managed credentials through child stdin to the controller-owned Python settings
  installer. `TartCommand::expr` constructs an expression without logging it.
  The installer reads JSON from stdin and writes private guest configuration
  files (0600, or root/group-readable 0640 where required); it never prints the
  payload. The provider replaces child errors with a fixed message rather than
  formatting the stdin-bearing expression. The reported sink is process input,
  not a log write. Arbitrary user-selected commands can intentionally print input;
  that is outside the reported controller-owned installer flow.
- **#11, generated password notice, false positive:** the formatted operands are
  the validated service name and secret-file path. The generated password is
  written to the private secret file and returned to its caller, never interpolated
  in the log message. File-path disclosure is not password-value disclosure.

## Cryptographic-value findings

| Alert | Exact location | Classification | Evidence and trust boundary |
|---|---|---|---|
| #15 | `rust/vm/src/commands/packages/access.rs:260` | Test-only signing key | `#[cfg(test)]` at access.rs:240; in-memory client-environment fixture for Docker/Tart endpoint and capability construction. No provider operation or deployed credential is created. |
| #16 | `rust/vm/src/commands/packages/access.rs:312` | Test-only signing key | Same cfg(test) boundary; signs and verifies fixture capabilities to assert distinct guest gateways retain the project principal (340–345). |
| #17 | `rust/vm/src/commands/packages/access.rs:352` | Test-only signing key | Same cfg(test) boundary; verifies v2 canonical-repository binding using fixture client variables. |
| #18 | `rust/vm-auth-proxy/src/crypto.rs:168` | Test-only password | `#[cfg(test)]` at crypto.rs:162; encryption roundtrip fixture using generated salt (169), no persistent production secret store. |
| #19 | `rust/vm-auth-proxy/src/crypto.rs:184` | Test-only password | Same cfg(test) boundary; first test key encrypts a fixture value, and the distinct second test key must reject decryption (189–192). |
| #20 | `rust/vm-auth-proxy/src/crypto.rs:186` | Test-only password | Same negative test; deliberately wrong second password verifies AES-GCM authentication rejects a different key (192). |
| #21 | `rust/vm-package-work/src/server/tests.rs:202` | Test-only signing key | File included only by server.rs:314–315 #[cfg(test)]. Fixture router/TestServer and TempDir Store at tests.rs:175–188; signs a test project capability to exercise scoped workflow authorization. |
| #22 | `rust/vm-package-work/src/server/tests.rs:419` | Test-only signing key | Same cfg(test) boundary. TempDir/TestServer at 403–417; signed capability without a tool attestation must be rejected (423–429). |
| #23 | `rust/vm-package-work/src/server/tests.rs:449` | Test-only signing key | Same isolated test router; signs an exact tool-source attestation and proves attacker-supplied registration fields cannot replace the signed identity (453–463). |
| #24 | `rust/vm-package-work/src/server/tests.rs:504` | Test-only signing key | Same cfg(test) boundary. TempDir/TestServer at 473–487; v1 capability must not authorize a canonical-workspace release (518–525). |
| #25 | `rust/vm-package-work/src/server/tests.rs:528` | Test-only signing key | Same fixture; signs an intentionally wrong canonical repository to test authorization rejection. No real server receives it. |
| #26 | `rust/vm-package-work/src/server/tests.rs:546` | Test-only signing key | Same fixture; signs matching canonical repository to test that checkout principal comes from authenticated claims, not spoofed request fields. |
| #27 | `rust/vm-package-work/src/server/tests.rs:652` | Test-only signing key | Same fixture; signs project-b capability and asserts unauthorized cleanup of project-a checkout (657–667). |
| #28 | `rust/vm-packages/src/credentials.rs:257` | Test-only signing key | `#[cfg(test)]` at credentials.rs:230; tests consumer-bound capability roundtrip and tamper rejection (258–267). |
| #29 | `rust/vm-packages/src/credentials.rs:265` | Test-only wrong signing key | Same cfg(test) boundary; wrong-key verifier intentionally must reject a capability signed with the other fixture key (264–266). |
| #30 | `rust/vm-packages/src/credentials.rs:272` | Test-only signing key | Same cfg(test) boundary; tests canonical repository normalization, modified payload and version rejection (273–287). |
| #31 | `rust/vm-packages/src/credentials.rs:292` | Test-only signing key | Same cfg(test) boundary; tests v2 exact tool-source attestation binding, no runtime credential setup. |
| #32 | `rust/vm-auth-proxy/src/crypto.rs:40` | False positive: overwritten output buffer | Zero initialization only allocates a 32-byte PBKDF2 output buffer. The unconditional pbkdf2_hmac call at 41 overwrites every byte from the supplied password and salt before Aes256Gcm::new_from_slice reads it at 44. No branch uses the zero buffer as a key. |
| #33 | `rust/vm-auth-proxy/src/crypto.rs:99` | False positive: overwritten RNG buffer | Zero initialization only allocates the salt buffer. Unconditional OsRng.fill_bytes at 100 fills all 32 bytes before return at 101. OsRng fails by panic if randomness cannot be obtained; it does not return the zero initializer as a fallback salt. |

## Production credential paths checked

- Auth-proxy storage reads a per-installation persisted master password at `rust/vm-auth-proxy/src/storage.rs:82` and the stored salt at 83–86. New master passwords use `OsRng.fill_bytes` at `crypto.rs:139–141`, not the test literals; new storage gets its random salt at `storage.rs:215–217`. Encryption creates a fresh OS-random nonce at `crypto.rs:52`.
- CLI capability issuance receives `files.agent_signing_key()` at `rust/vm/src/commands/packages/access.rs:49`. That reads its persisted credential at `files/credentials.rs:65–66,117–125`; new appliance credential files use `generate_random_password(48)` at `files/definition.rs:34–39`. None of the test literals serve as fallback credentials.
- Deployed package-work requires `PKG_WORK_AGENT_SIGNING_KEY` from its process environment at `rust/vm-package-work/src/main.rs:56`; the test router directly supplies its own fixture credentials. The service validates minimum signing-key length at `server.rs:82`.
- Signing/verification APIs accept caller-provided key material (`rust/vm-packages/src/credentials.rs:132–164,202–215`); hard-coded keys occur only below the `#[cfg(test)]` boundary at line230.

## Suggested dispositions for owner review

- #15–31: `used in tests`, with the per-alert compile-time exclusion and test purpose above.
- #32–33: `false positive`, because the flagged initializers are replaced before use by PBKDF2/OS randomness.

This conclusion is limited to the reported hard-coded-value flows, not a claim that the entire cryptographic/storage design is independently audited. Existing tests already cover different-key rejection, random salt/token differences, persisted master-key reuse and tamper-evident capabilities. No additional test run is necessary for this read-only classification.

## Filesystem findings

## Alert-by-alert evidence

- **#2 — source/submission.rs:14, create_dir_all(root).** The route handler `server/bundles.rs::upload_submission` treats the request checkout ID as a database lookup key via `Store::authorize_lease`, which verifies an active matching lease and consumer before returning the stored checkout. `SourceManager::submission_staging_path` obtains the directory through `agent_root`, which passes checkout_id through `managed_component` / `vm_packages::validate_managed_id`. That validator accepts only 1–160 ASCII letters, digits, `_`, and `-`. Absolute paths, separators, drive prefixes, dots/traversal and percent escapes are rejected before any filesystem mutation. `SourceManager.root` comes from the configured service data directory, not the upload body or query.
- **#3 — source/submission.rs:16, create_dir_all(uploads).** Same checked root as #2, with the literal component `uploads`. The returned bundle filename consists of 16 server-generated ASCII alphanumeric characters and the literal suffix `.bundle`. Request bundle bytes are file content, not a filename.
- **#4 — store/receipt.rs:61, create_dir_all(releases).** The actual alerted directory is `Store.root()/receipts/releases`, all literal components below the startup data directory. The incoming submission can trigger receipt projection but cannot set `Store.root`. Individual release filenames are derived from service-generated submission/release IDs, not the request body’s path text.
- **#5 — store/receipt.rs:70, create_dir_all(consumers).** The actual alerted directory is the literal `Store.root()/receipts/consumers`. Consumer registration is controller-authenticated. Consumer names pass `validate_label`, which prohibits any `..`, leading slash, backslash, colon and NUL; ordinary/scoped labels cannot escape this root. The confirmed scoped-label projection correctness issue below is fixed by encoding the filename as one component.
- **#6 — store/receipt.rs:79, create_dir_all(rollouts).** Literal `Store.root()/receipts/rollouts`. Rollout IDs are generated from `branch_component(package)`, the server date and sequence. `branch_component` replaces everything except ASCII alphanumeric characters with `-`; the caller cannot select a directory.
- **#7 — store/receipt.rs:88, create_dir_all(tools).** Literal `Store.root()/receipts/tools`. Tool receipt IDs are server-generated `tool-receipt-{:08}` sequence values. Request content serialized inside receipts does not become a path component.
- **#8 — temporary_cleanup.rs:4, remove_file(path).** Upload cleanup receives the staging path generated as described in #2/#3. Rollout cleanup receives a stored, service-generated rollout ID and a worktree checked for exact equality with `root/rollouts/<id>/source`, then a literal uploads directory plus random filename. Other call sites clean generated unpublished bundle files: release source digests are computed from bytes, submission IDs pass managed-component validation, and binary build commits pass `ToolSourceManifest::validate` (full 40/64 lowercase hex object ID). The generic cleanup helper never consumes a raw HTTP filename.
- **#9 — vm-core/file_system.rs:9, path.is_dir().** This is a shared atomic-file-writing primitive, not an HTTP path parser. In the reported upload flow, receipt paths are constrained as above; submission diff filenames come from `bundle_head`, which accepts only 40/64 hexadecimal Git object IDs, under the validated checkout root. Incoming diff/receipt text is supplied as the separate content argument. The destination is replaced with an adjacent `NamedTempFile::persist`; an existing destination symlink is replaced without modifying its target. This primitive deliberately permits caller-selected paths for other authorized local filesystem operations; adding a global fixed-root restriction would break those callers without fixing a demonstrated HTTP vulnerability.

## Regression coverage added

- `source::submission::tests::submission_staging_rejects_path_components_before_creating_directories`: checks empty/dot/traversal/Unix absolute/Windows drive/backslash/percent-encoded/slash IDs reject before even creating the root; valid IDs allocate distinct names under the exact managed uploads parent without creating bundle files.
- `file_system::tests::atomic_write_replaces_a_destination_symlink_without_writing_its_target` (Unix symlink semantics): replacement leaves an outside sentinel unchanged and replaces the symlink with the intended regular file.

These conclusions assume the appliance-owned data directory and service configuration are trusted. An administrator who can alter private on-disk workflow JSON, replace ancestor directories with symlinks, or change service startup paths already controls the service's filesystem authority. No stronger local-race or hostile-administrator guarantee is claimed.

## Related correctness regression confirmed and fixed

`validate_label` accepts scoped consumer names such as `scope/name`, but the consumer receipt projection directly appends `<name>.json` without creating that intermediate subdirectory. Such a valid label can cause a stale projection / missing-parent error. This does not bypass containment (`..`, absolute, Windows drive/separator syntax are rejected), and is not the directory-creation path reported by #5. A focused regression reproduced the failure before changes: `Internal("No such file or directory ... receipts/consumers/team/.vm-write-...")` for accepted consumer `team/app`. Consumer projection filenames now use the same lossless percent-encoding used by consumer client request paths. Scoped labels remain one filesystem component, ordinary `team-app.json` remains unchanged, and `team/app`, `team-app`, and `@team/app` cannot collide. No migration or compatibility path was added. Regression checks exact persisted content and successful service reopen.

## Validation result

Passed `cargo test --manifest-path rust/Cargo.toml -p vm-package-work submission_staging_rejects_path_components_before_creating_directories` (1 focused test) and `cargo test --manifest-path rust/Cargo.toml -p vm-core file_system::tests` (4 tests). Formatting applied to owned crates; `git diff --check` passed. Initial path triage introduced tests only; the subsequent scoped-consumer fix is described above. No commit or GitHub disposition made.

Scoped-consumer regression after fix: `cargo test --manifest-path rust/Cargo.toml -p vm-package-work scoped_consumer_receipts_materialize_and_survive_reopen` PASSED (1 test; previously failed). Formatting applied and `git diff --check` passed. All owned source is frozen.
