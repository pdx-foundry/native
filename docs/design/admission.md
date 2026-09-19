# Exact-target capability admission

`Engine::open(OpenRequest)` identifies the hinted executable and returns an opaque `EngineContext`.
It does not launch a game, attach a debugger, or read private evidence. Existing `Engine.replay`
remains independent of installation access.

## Identification and scope

Hints can name an executable, `stellaris.app`, or installation directory. Directory resolution
checks only `stellaris.app/Contents/MacOS/stellaris`, `Contents/MacOS/stellaris`, `stellaris.exe`, and
`stellaris`. Multiple candidates are ambiguous, even if one is known. There is no automatic Steam
search, platform selector, version fallback, or adapter override.

The `object` reader validates the selected executable. Native selects exactly one ARM64 slice from
a universal Mach-O image, checks its architecture against the image header, and hashes both the
complete image and selected slice. Thin Mach-O ARM64 and PE x64 identities can be inspected; only
M45-observe currently has an exact catalogue entry. Other architectures/formats are unsupported;
unregistered identities are unknown. Malformed images and access failures remain separate errors.

The host-neutral M45-observe record refers to one typed recipe. The composer resolves shared
binding declarations and machine/strategy revisions once. The compiled macOS ARM64 resolver supplies the implemented strategy and its actual worker package.
Other hosts refuse it. Composition carries the selected machine, bindings, content prerequisites,
and executable/slice identities into shared execution. See [registry queries](live-observations.md).

## Admission authority

`capability(&CapabilityRequest)` reports qualification, availability, declared and accepted bounds, blocking
reasons, accepted record identities, and immutable evidence references. Requests name one registry.
`traditions` and `tradition_categories` have declared methods; other names are outside support.
The operation retrieves entry keys at initial loader return, before validation. It does not expose
fixture selection, hook windows, field values, gameplay, or rule conclusions.

Qualification requires a bundled, unwithdrawn acceptance matching the complete composition,
relevant content, and the whole requested scope. An acceptance for one registry cannot grant support for another. Composition identity includes executable and slice hashes, recipe,
method, machine and strategy revisions, and hashes of shared binding declarations. The exact debugger identity must also match acceptance. The source
records and withdrawals are the authority; records supplied by a caller or capture are not loaded.

The production acceptance list contains the reviewed replacement registry qualification. Ordinary
installation-backed admission independently requires the `production` feature in both processes. The
verified SDK-483 experiment alone does not qualify this Rust implementation. A recipe alone remains
incomplete. Qualification and availability are independent: an accepted request can still be blocked
by current inputs or host/tool prerequisites. Other compositions do not inherit an acceptance.

Relevant content is snapshotted from `launcher-settings.json` and the complete file inventories
under `common/tradition_categories` and `common/traditions`. These are the retained prototype's
content boundary, not a promise of complete game-content coverage. Missing or unreadable inputs
are unavailable. Content symlinks are rejected. Each query rechecks the executable and content
inventory, including additions/deletions. A failed integrity check permanently invalidates the
context; restoring the old bytes requires opening a new context.

Admission never opens historical evidence bytes. Accepted evidence references remain provenance
even when their artifacts are absent. Replay and qualification review still need those bytes.
Capability queries capture no new observations and issue no execution permit.

## Synthetic contexts and release checks

The non-default `test-support` feature exposes `test_support::engine(SyntheticCase)`. Named fixed
scenarios feed the ordinary admission evaluator and return the ordinary context API. All inputs
are in memory; results retain synthetic origin. No factory accepts paths, arbitrary records, or
executable bytes. Context source is private, with no rebinding API or path to live execution.

Official builds use `cargo build --release --features production`. Compile guards reject
`production` with `test-support` or `maintainer-tools`, and the build script rejects release-profile
`test-support`. The maintainer feature now exposes a separate candidate lifecycle library API; see [consumer-hosted lifecycle](lifecycle.md). Native distributes no supervisor executable.
`--all-features` is intentionally an invalid build combination.

`tools/check-admission-boundary.py` builds production, checks its resolved feature set, and requires
the forbidden builds to fail for their intended diagnostics. Temporary compilation probes prove
that a shared-operation sibling cannot import private binding descendants, default consumers
cannot import the factory, and callers cannot construct contexts. CI runs these checks plus both
default and test-support suites on macOS, Windows, and Linux. No game or private bundle is needed.
