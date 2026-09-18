# PDX Native specification

Status: local implementation specification for review. Based on [Specify PDX Atlas and its engine-derived rule database](https://linear.app/unnamed-system/issue/SDK-470/specify-pdx-atlas-and-its-engine-derived-rule-database) and its accepted decisions, including the 2026-09-17 sequencing change. This document specifies required behavior; it does not certify an implementation or approve a release.

## Problem Statement

Atlas needs repeatable engine evidence to derive Stellaris scripting rules. Existing experiments mix useful engine observations with executable addresses, memory layouts, compiler patterns, launch workarounds, and platform-specific process control. If Atlas depends on these details, each game update becomes an Atlas port and native assumptions spread across projects.

Atlas must be able to ask the same engine questions on every qualified target. It must know when observations are incomplete, unavailable, or outside their demonstrated limits. A successful launch, a discovered name, and a valid rule conclusion are different results. Current bounded prototypes establish useful mechanisms, but do not establish complete tradition coverage, stable cross-platform support, or inexpensive maintenance.

## Solution

Build PDX Native as a Rust engine-integration library, with Atlas as its first consumer. Expose useful engine operations and structured observations through one public boundary. Native owns all platform and game-version operations, including static native analysis, target qualification, process lifetime, hooks, and evidence capture.

Atlas supplies engine-level questions, fixtures, and time bounds. Native discovers the environment, selects qualified implementations, and returns normalized observations, exact limitations, and evidence references. Atlas interprets those observations into rule claims and assembles offline snapshots. Offline tools consume Atlas snapshots without installing Native or Stellaris.

**Atlas has no platform or game-version implementation knowledge.** Its extraction code does not select adapters, decode native data, compare game version strings, or choose a platform-specific path. Exact target identity remains in Native-owned provenance that Atlas can retain and forward unchanged. Native supplies the qualification and applicability decisions needed to use that evidence; Atlas does not reconstruct them from target metadata.

## User Stories

1. As an Atlas developer, I want one engine interface, so that extraction logic works unchanged across qualified platforms and game builds.
2. As an Atlas developer, I want Native to discover an installation or accept a location hint, so that I do not maintain platform-specific installation paths.
3. As an Atlas developer, I want Native to select and verify the target, so that I do not choose executable architectures or adapters.
4. As an Atlas developer, I want capability results with explicit reasons and limits, so that missing native support becomes a visible extraction gap.
5. As an Atlas developer, I want to enumerate engine registry candidates without config seeds, so that I can discover fields and families missing from config.
6. As an Atlas developer, I want loader, owner, and reader relationships, so that I can distinguish a candidate name from an established authored definition.
7. As an Atlas developer, I want engine command and scope declarations with their source, so that I can distinguish declared information from demonstrated behavior.
8. As an Atlas developer, I want source-correlated field observations, so that evidence identifies the fixture, field, owner, and processing stage involved.
9. As an Atlas developer, I want reference-resolution observations with conditional alternatives, so that I do not turn partial evidence into an unconditional rule.
10. As an Atlas developer, I want observations of shared readers, so that one supported mechanism can inform several rule properties.
11. As an Atlas developer, I want parser storage, validation, and runtime outcomes recorded separately, so that engine recovery does not imply valid input.
12. As an Atlas developer, I want observations of scope type and actual availability, so that a pointer or declaration does not become a false scope guarantee.
13. As an Atlas developer, I want to supply fixture files and observation windows, so that I control the experiment without controlling hooks or launch timing.
14. As an Atlas developer, I want parsing-only jobs, so that registration and loading experiments do not require a saved world.
15. As an Atlas developer, I want prepared-script operations and normalized state reads when qualified, so that I can test runtime questions without direct engine calls.
16. As an Atlas developer, I want unknown conditions and missing joins preserved, so that supported observations remain useful without concealing gaps.
17. As an Atlas developer, I want opaque subject handles, so that I can request related observations without knowing class names, addresses, or offsets.
18. As an Atlas developer, I want exact evidence identities attached automatically, so that every claim can retain its supporting provenance.
19. As an Atlas developer, I want retained evidence replay, so that I can develop and check extraction without launching a game.
20. As an Atlas developer, I want replay to remain distinct from fresh capture, so that historical success does not certify the current installation.
21. As an Atlas developer, I want missing and damaged evidence reported explicitly, so that inaccessible evidence does not become an empty successful result.
22. As an Atlas developer, I want bounded execution and cancellation, so that a stuck game cannot leave extraction waiting indefinitely.
23. As an Atlas developer, I want activation, completion, and disposal reported separately, so that I can assess evidence and cleanup independently.
24. As an operator, I want isolated profiles and owned resources, so that experiments do not change my ordinary game profile or unrelated processes.
25. As an operator, I want cleanup to survive observation-worker failure, so that a failed probe does not abandon its game process.
26. As an operator, I want incomplete attempts and cleanup failures retained, so that later successes do not erase failure evidence.
27. As a Native maintainer, I want exact target checks before native operations, so that game updates cannot silently reuse stale assumptions.
28. As a Native maintainer, I want qualification per operation and target, so that one successful method does not imply universal support.
29. As a Native maintainer, I want frozen methods tested on unfamiliar cases, so that reuse is demonstrated rather than inferred from training examples.
30. As a Native maintainer, I want native adaptation and Atlas semantic changes recorded separately, so that maintenance cost and ownership remain clear.
31. As a Native maintainer, I want pinned source releases and reproducible evidence manifests, so that another producer can identify what actually ran.
32. As an Atlas maintainer, I want qualification changes to identify affected evidence, so that Atlas can requalify or correct the relevant claims.
33. As an offline tool maintainer, I want Native confined to evidence production, so that ordinary builds and authoring need neither native tooling nor game execution.

## Implementation Decisions

### 1. Authority and module boundaries

The accepted architecture has three deep modules: Native, Atlas extraction, and Atlas rule assembly. Native's internal components are implementation boundaries, not additional interfaces that Atlas must coordinate.

| Module | Owns | Provides to its caller |
| --- | --- | --- |
| Native public engine service | Session and request contract; normalized results; capability and evidence applicability queries | Engine operations, observations, explicit limits, opaque identities |
| Native target and qualification registry | Installation discovery; executable, architecture, and relevant content identity; adapter selection; operation qualification; update detection | A fixed internal target binding and qualified capabilities |
| Native execution supervisor | Private profiles; process/resource ownership; worker transport; deadlines; cancellation; independent disposal | Bounded job execution and separate disposal evidence |
| Native adapters and analysis methods | Binary decoding; symbols/patterns; signatures and layouts; object lifetime; hooks; engine calls; normalized native relationships | Evidence-backed operations for a particular qualified target |
| Native evidence capture and replay | Run identities; source and artifact hashes; stream integrity; retention manifests; replay validation | Retrievable evidence and results whose capture/replay origin is explicit |
| Atlas extraction | Property questions; fixtures and probe matrices; evidence interpretation; rule qualification; coverage obligations and gaps | Supported claims and unresolved properties |
| Atlas rule assembly | Claim consistency; rule identities; conditional rules; documentation; snapshot contract and publication | Deterministic offline snapshots with evidence references and coverage |
| Offline consumers | Project inputs; snapshot pinning; rule application; derived types; authoring advice and diagnostic severity | Author-facing tools |

Dependencies run from Atlas extraction into Native's public service. Native does not import Atlas's rule model, coverage ledger, or snapshot assembler. Atlas does not import adapters, native analysis helpers, debugger machinery, or process-control helpers. Rule assembly consumes evidence-bearing claims without opening an engine session.

Native establishes what was called, read, or observed under stated conditions. Atlas decides what that establishes about a rule. Native method qualification and Atlas rule qualification remain separate even when they share artifacts.

### 2. Public contract and target opacity

Use a Rust library boundary consistent with the current Native and Atlas repositories. Exact method names, crate layout, and private worker transport may evolve without changing these responsibilities. The map did not freeze production signatures; the following operation families are requirements for the implementation, not claims that a released API already exists.

| Operation family | Atlas supplies | Native returns |
| --- | --- | --- |
| Open engine context | Optional installation location hint | Opaque context and evidence-context identity, or a precise discovery/qualification failure |
| Inspect capabilities | Engine-level operation and requested scope/window | Qualification status, supported bounds, missing prerequisites, and evidence references |
| Discover subjects | Registry/command kind or an opaque discovered subject | Candidates, established relationships, discovered subjects, and explicit search limits |
| Analyze subject | Opaque subject and requested relationship, such as reader or reference resolution | Normalized observations, stages, alternatives, established joins, and unresolved joins |
| Observe fixture | Fixture files, engine-level observations, bounded window, deadline | Activation, observations, completion/integrity, evidence, and disposal results |
| Execute prepared script or read state | Script, semantic context/handles, required observation, deadline | Correlated execution/state observations within qualified operation limits |
| Cancel or close | Opaque job/context identity | Cancellation outcome and independently confirmed or unconfirmed disposal |
| Inspect applicability or replay evidence | Native-issued evidence/context references or retained bundle | Qualified applicability relation or replay result, with precise limits and failures |

An engine context need not launch a game. Static analysis can run without a process; registration/parsing jobs need not load a world. Native determines the required phases and execution mechanism from the operation contract.

Native fixes target and adapter selection once for a context, then verifies that relevant inputs still match before execution. A changed target invalidates the binding for affected operations. A caller cannot force an unqualified adapter or request the nearest supported version. Ambiguous installation discovery returns a location-selection result; it does not silently choose a different installation.

Atlas may inspect semantic capabilities and explicit observation gaps. It may compare opaque identities for equality, retain them, and request Native's applicability assessment. It must not parse them to infer architecture, version compatibility, or which implementation to call. Native supplies authoritative compatibility relationships; Atlas may narrow the applicability of its own claims but cannot widen it beyond their qualified evidence.

Human-readable build/platform labels and exact identities may appear in Native diagnostics and evidence exports. They are provenance, not Atlas control inputs. Snapshot consumers can still select an explicitly applicable game target; this does not make Atlas extraction responsible for native version handling.

No request accepts addresses, offsets, native signatures, assembly, injected native code, debugger commands, platform launch switches, or timing workarounds. Missing behavior requires a Native extension or an explicit gap. There is no raw-native escape hatch for Atlas.

### 3. Observation and evidence model

Each result must carry enough information to assess the exact observation without inspecting native implementation details:

- Contract identity, request/run identity, and capture or replay origin.
- Opaque evidence-context identity, automatically linked to exact executable/architecture/platform, relevant content, fixture, Native source/artifact, and method revisions.
- Engine subject, processing stage, source location when established, and opaque owner/input/destination handles.
- Observed facts and normalized relationships, each with its supporting evidence reference and stated basis.
- Conditional alternatives, including no-write, missing-object, alternate-input, and unresolved-condition cases where relevant.
- Established joins and missing joins between registration, loader, owner, token, reader, destination, initialization, resolution, validation, and use.
- Native capability/method qualification, observation limits, and precise gaps.
- Activation and stream-completion evidence for live observation, plus a separate disposal result for owned processes.

Exact native details live in the producer evidence manifest. Atlas can preserve that manifest as an opaque attachment and use stable evidence references without interpreting its target fields. Native's contract and serialization versions are distinct from game versions. Reject an incompatible contract explicitly; do not silently discard unfamiliar fields that change evidence meaning.

Target-local handles are opaque and valid only within their declared context. Native rejects handles from a different context and expired live-object handles. Such handles are not stable Atlas rule identities. Native must not claim cross-build identity correspondence from similar symbols, addresses, or offsets.

Distinguish these dimensions rather than compressing them into one success flag:

| Dimension | Required distinctions |
| --- | --- |
| Native support/qualification | Qualified within bounds; outside declared support; qualification incomplete |
| Observation availability | Available; unavailable with reason |
| Knowledge | Established observations; partial observations with gaps; unknown answer |
| Capture completion | Complete bounded window; incomplete stream/window; worker lost; cancelled or timed out |
| Hook activation | Demonstrated before the required phase; not established/unavailable |
| Disposal | Confirmed; unconfirmed; not applicable when no process was created |

A complete bounded capture is not complete game knowledge. A successful empty result requires evidence that the requested window or search completed and contained no matching observations. Access failure, an unresolved hook, record loss, and an unexamined property cannot produce that result. An unavailable operation does not mean the game forbids a construct.

Preserve parser storage, engine validation, and runtime outcome separately. Scope pointer presence does not establish scope availability. Candidate reference classes do not establish collection ownership or lookup semantics. Unsupported native conditions remain unknown; Atlas does not decode them itself.

### 4. Lifecycle and isolation

Before launch, Native validates required capabilities and inputs, establishes resource ownership, prepares an isolated profile, and records the intended observation phases. It tracks resources from partial launch onward. It must not modify the ordinary profile or kill a process it does not own.

Live jobs establish three independent results:

1. **Activation:** required observations were active before the relevant engine phase, with an ordering witness. Scheduling a hook, suspending a process, waiting a fixed delay, or reaching a world-ready marker is insufficient.
2. **Observation completion:** the requested window ended, sequence and totals agree, required source/owner joins are present, and the terminal witness exists. Each completeness claim names its window.
3. **Disposal:** the independent supervisor confirms that the owned process exited and its platform ownership resources were released; child reaping is included where applicable.

The supervisor owns the game independently of the observation worker. Worker failure must not remove the ability to dispose of the game. Cancellation, timeout, failed activation, process crash, and partial launch retain their evidence and end with a bounded disposal attempt. Unconfirmed disposal is reported and prevents reuse of the affected isolation context.

Do not replay an operation whose completion is uncertain. A new attempt uses a fresh identity and isolated resources after prior ownership is resolved. No blind retry of a potentially mutating engine operation is permitted. Retain failed attempts even when a later attempt succeeds.

Initially serialize game-owning jobs where the adapter cannot qualify concurrency. Reject a conflicting live instance with an explicit reason. Recovery after loss of the supervisor itself, general concurrent sessions, and universal invisible/background operation remain unqualified extension work. OS process termination must not be described as a graceful in-game exit.

### 5. Shared native methods

Keep decoding, value provenance, bounded control-flow analysis, known-call summaries, owner joins, and supported reader patterns inside Native. Adapters bind those methods to qualified targets. Do not build an independent handwritten answer table for each command or field.

Discovery returns candidates separately from qualified relationships. It must retain unobserved candidates, unknown calls, unresolved helper paths, and incomplete member inventories. Engine-only discovery is evaluated without config or a supplied field list; config/content comparison may follow a frozen result.

Documentation output establishes what the engine declares. Static analysis can establish traced relationships within a qualified method. Live instrumentation establishes what happened under tested conditions. Use the methods appropriate to the claim; neither universal live testing per property nor a general decompiler is required.

An unfamiliar instruction shape, unresolved callee, clobbered value, or unproved owner narrows or stops a result. Conditions and outcomes must be traced together. A method extension is frozen before testing a genuinely unfamiliar case. Previously failed cases used during development are no longer held-out tests.

Any manual exception records its exact claim, supporting evidence, conditions, current obstacle, and possible removal route. It must not be presented as automatic extraction or become a hidden second rule authority.

### 6. Atlas's first consumer path

The first implementation path is target qualification → independent ownership/disposal → bounded early observations → retained capture/replay → Atlas interpretation. Use the accepted tradition/category experiments as initial consumer fixtures. They establish the interface path, not the entire first release.

Atlas's declared first release needs evidence for tradition and category structures, bonuses, references, conditions, modifiers, effects, swaps/inheritance, weights, localisation/tooltips, icons, and relationships to existing tree templates. Its bounded script vocabulary includes `always`, `has_country_flag`, `has_tradition`, AND/OR/NOT, country-flag changes, resource changes, conditional branches, additive/multiplicative naval capacity, and literal values/conditional weights in the required country contexts.

Native must support the engine observations needed to qualify those promises as implementation grows:

| Atlas question | Native responsibility | Atlas responsibility |
| --- | --- | --- |
| Which authored definitions and fields exist? | Discover candidates and establish loader/owner/token/reader joins with limits | Decide coverage obligations and rule subjects |
| What happens when fields are omitted, repeated, malformed, or conditional? | Observe stages, storage, diagnostics, conditions, and relevant execution | Derive bounded structural and conditional claims |
| Which definitions do references select? | Establish lookup/owner relationships and conditional outcomes | Publish reference categories and relationships; leave project values to consumers |
| Which shared numeric, command, modifier, or weight reader is used? | Expose established reader behavior and unresolved paths | Compose reusable rule definitions and preserve gaps |
| Which script contexts are available? | Observe actual scope type/availability and relevant identity witnesses | Publish supported scope constraints and document identity/runtime behavior |
| What affects tree layout, adoption, swaps, and completion? | Execute qualified prepared operations and expose correlated state/behavior observations | Design fixtures and assess the exact rule or documented outcome |
| Which files and duplicate definitions were used? | Observe mounted selection, loader phases, and duplicate processing where qualified | Derive supported naming/loading relationships without generalizing beyond evidence |

The bounded bonus-reference prototype is suitable for the first vertical integration check. Add discovery and shared-reader cases without treating that two-field proof as whole-category validity. Inventory size and successful examples cannot establish full coverage.

Atlas owns fixture meaning, including required content relationships. Native owns mounting, isolation, native execution, and recording the actual content boundary. Native must not turn fixtures or installed content into an authoritative shipped content catalogue.

### 7. Target qualification and maintenance

Begin qualification with the pinned native Apple Silicon Stellaris 4.5 beta identified by retained evidence. Verify actual executable and content identities before use. These are accepted target-policy requirements, not an assertion about which game release is current.

The first stable scope requires requalification on stable 4.5 for Apple Silicon and Windows x64, with Mac as the primary evidence-production environment. Linux and Intel Mac are outside the initial promise. Maintain one qualified stable release at a time; retain older evidence and original applicability. A new patch does not inherit support automatically.

Qualification is per exact target, operation, method revision, and relevant content dependencies. Native detects changed identities and refuses stale assumptions during ordinary extraction. Separate maintainer qualification work can investigate new targets. Requalification can use relevant reruns or demonstrated unchanged dependencies; unrelated passing checks and matching version labels are insufficient.

Freeze Atlas extraction logic for the Mac/Windows comparison and a second distinct executable on one platform. Routine port work stays in Native. If a genuinely new engine concept requires a public-contract change, record it as an explicit amendment. Investigate semantic discrepancies before proposing platform-specific rules.

Measure initial tooling, routine updates, and exceptional repairs separately. Record shared-method work, target adaptation, fixture corrections, Atlas interpretation, human attention, agent effort, and unavailable measurements. Run duration and output counts are not maintenance-cost evidence. No numeric maintenance guarantee has been accepted.

### 8. Evidence, replay, and releases

Atlas pins an exact Native source release. Record source revisions, dependency/tool versions, built artifact identities, executable and relevant content identities, fixture inputs, method revisions, and operation bounds in producer provenance. Native source releases and Atlas snapshots can advance independently through explicit contract compatibility and dependency pins.

Retain immutable capture bundles with hashes, portable archive-relative locations, reproduction prerequisites, and identity-to-location mappings. Supporting evidence, relevant failures, corrections, and qualification records remain retrievable. Original absolute paths and expiring URLs cannot be the only locators. A missing artifact is an evidence-access failure, not a new game fact.

Replay checks retained bytes and derivations. It must not silently launch the game or imply fresh target qualification. Fresh capture requires explicit target prerequisites and a new run identity. Preserve original captures when replay emits new derived output.

Native supplies invalidation/requalification information for its methods and evidence. Atlas owns withdrawing or correcting affected published rule claims and snapshots. Preserve historical evidence even when superseded. Shared storage does not merge these authorities.

Initially build Native from pinned source and run maintainer-started jobs on the existing producer machines. Native prebuilts and automatic machine scheduling are deferred. Public original source and compact authored summaries remain separate from private raw game material. Durable evidence policy requires a private remote copy and a second local copy, with hash and restore verification. Publication remains maintainer-approved.

## Testing Decisions

### Primary test seam

Test externally observable behavior through Native's public engine-operation interface. Jackson confirmed this test seam during specification review. Use the same Atlas request flow for retained-evidence replay and live qualified adapters. Replay identifies itself and establishes only retained derivation behavior. Live qualification is required for claims about target access, early ordering, isolation, and disposal.

Prefer this single consumer seam over tests coupled to private functions or adapter call order. Focused internal tests are justified for unsafe decoding or supervision failures that cannot be induced safely through the public seam; they do not replace public-contract qualification.

### Required acceptance checks

1. **Boundary:** the same Atlas caller issues engine-level requests against qualified adapters without platform/version branches, native constants, or adapter imports. Target details pass through only as opaque provenance. A fake/replay provider can test this shape but cannot certify portability.
2. **Qualification:** changed executable/content identity, unavailable operation, incomplete qualification, incompatible contract, foreign handle, and expired object each return the declared result without executing stale assumptions.
3. **Observation integrity:** wrong owners, clobbered values, changed branch conditions, unresolved calls, missing joins, and missing evidence preserve precise gaps. No partial result becomes an unconditional reference or complete schema.
4. **Early phases:** normal, deliberately missing/late hook, dropped-record, missing-terminal, and worker-loss cases distinguish activation, observation completion, and disposal. Ordering witnesses establish the required phase relation.
5. **Lifecycle:** partial launch, timeout, cancellation, crash, and observation-worker termination reach bounded cleanup through the independent owner. Verify ordinary profiles and unrelated processes remain unchanged. Preserve unconfirmed disposal instead of overwriting it.
6. **Replay:** verify bundle and artifact identities, reproduce bounded normalized observations, reject corrupt/missing evidence, and establish that replay does not launch the game. Failed capture evidence remains readable.
7. **Shared-method transfer:** freeze a method and apply it to unfamiliar cases with positive and negative controls. Record modifications and failures instead of silently updating the frozen baseline.
8. **Atlas integration:** carry an established tradition field through Native observations, Atlas claims/snapshot, and an offline experimental consumer. Include valid/invalid input, a conditional or shared-reader case, and absent evidence. Assert both supported behavior and refusal to claim whole-slice completeness.
9. **Portability:** run the frozen Atlas extraction flow on corresponding Mac/Windows targets and a second distinct executable. Keep content/architecture changes visible. Compare observations, limits, and intervention records; do not substitute ready-world adapter tests for extraction qualification.
10. **Release evidence:** report Native capability qualification separately from Atlas coverage. Unresolved properties required by a promised guarantee block that guarantee. Durable retention and reproducible generation are required in addition to passing examples.

### Prior art and current state

Reuse the retained reference-observation controls, early-observation normal/missing/incomplete/worker-loss scenarios, registry-discovery omission controls, registry-ownership replay, command/numeric reader controls, and the tradition-to-SDK experiment. These supply test cases and expected bounded outcomes, not a production test suite or transferable target certification.

At writing, the tracked executable is a Rust scaffold and there is no implemented public engine library or project test suite. Existing evidence tools verify imported archives and selected retained observations. The evidence index contains a pointer to a first-slice implementation document absent from this checkout; that pointer is not proof of implementation. Future checks must be runnable from a clean Native checkout, with game-free contract/replay checks separated from explicitly configured live qualification.

## Out of Scope

- Implementing Atlas rule extraction, its snapshot assembler, or consumer diagnostic policy inside Native.
- Shipping game/mod catalogues or supplying config-derived fallback answers for missing observations.
- A general decompiler, arbitrary native-code execution API, or a guarantee that every game property can be extracted automatically.
- The separate author-facing real-game testing framework, its assertions, discovery, and reporting. Reuse its native evidence without depending on its unfinished implementation.
- Full game-rule coverage or support for other games, Linux, and Intel Mac in the initial release.
- Universal concurrent sessions, supervisor-loss recovery, guaranteed invisible launch on every target, or unqualified engine stages.
- Atlas's later broad families, including audio, general interface/graphics formats, map/defines, descriptors, and broad callback/localisation inventories. Required tradition asset references and entry scopes remain first-release obligations.
- Scope changes, loops, variables, scripted-effect/scripted-trigger calls, and unchanged import of complete vanilla flexible-tree event chains in the first supported Atlas authoring path.
- Production SDK migration, config retirement, compiler implementation, prebuilt Native distribution, and automatic producer scheduling.

## Further Notes

The [technical design](../design/architecture.md) defines the proposed project layout, knowledge ownership, and exact-target composition inside Native.

### Handoff and release gates

Foundation implementation can begin now. This specification does not close open map investigations or reduce the agreed first-release scope. The latest map sequencing supersedes older records that required all experiments to finish before specification handoff.

| Work | Effect on Native and Atlas |
| --- | --- |
| [Measure Atlas extraction portability and update-maintenance effort](https://linear.app/unnamed-system/issue/SDK-485/measure-atlas-extraction-portability-and-update-maintenance-effort) | Run early; validates the unchanged Atlas boundary and remains a first-stable-release gate |
| [Validate extraction of field shapes and conditional constraints](https://linear.app/unnamed-system/issue/SDK-490/validate-extraction-of-field-shapes-and-conditional-constraints), [Validate nested command and control-block grammar extraction](https://linear.app/unnamed-system/issue/SDK-507/validate-nested-command-and-control-block-grammar-extraction), [Validate complete script scope-context and target rules](https://linear.app/unnamed-system/issue/SDK-495/validate-complete-script-scope-context-and-target-rules), and [Validate shared script expansion and parameter rules](https://linear.app/unnamed-system/issue/SDK-501/validate-shared-script-expansion-and-parameter-rules) | Early investigations may refine normalized observations and shared interfaces; they do not authorize assumed answers |
| [Validate complete registry reconstruction from shared rules](https://linear.app/unnamed-system/issue/SDK-506/validate-complete-registry-reconstruction-from-shared-rules) | Atlas composition through the emerging implementation must assess every promised obligation |
| [Qualify custom, nested, and late registry discovery](https://linear.app/unnamed-system/issue/SDK-509/qualify-custom-nested-and-late-registry-discovery) and [Qualify mounted file selection and duplicate registry rules](https://linear.app/unnamed-system/issue/SDK-510/qualify-mounted-file-selection-and-duplicate-registry-rules) | Preserve exact Native discovery/loading gaps and their composition dependencies |
| [Preserve Atlas planning and extraction evidence](https://linear.app/unnamed-system/issue/SDK-486/preserve-atlas-planning-and-extraction-evidence) | Local imports exist; private remote preservation remains outstanding |
| [Define release acceptance and assemble the Atlas specification](https://linear.app/unnamed-system/issue/SDK-480/define-release-acceptance-and-assemble-the-atlas-specification) | Owns the wider Atlas handoff; this local Native specification is an input |
| [Qualify the first Atlas release](https://linear.app/unnamed-system/issue/SDK-511/qualify-the-first-atlas-release) | Requires composition, target/maintenance evidence, durable evidence, and the complete promised consumer path; foundation implementation is not release acceptance |

Atlas retains the source-linked coverage ledger and individual owners for unresolved reference, numeric, modifier, weight, naming, and other properties. Native tracks the capabilities those investigations need. Neither project may silently mark a gap complete because another ticket closed.

### Governing records

- [Choose the extraction architecture and native-runner boundary](https://linear.app/unnamed-system/issue/SDK-475/choose-the-extraction-architecture-and-native-runner-boundary): module authority, first consumer, native isolation, and lifecycle contract.
- [Choose supported builds and the update-maintenance policy](https://linear.app/unnamed-system/issue/SDK-476/choose-supported-builds-and-the-update-maintenance-policy): exact-target policy and maintenance requirements.
- [Define evidence, uncertainty, and conflict handling for Atlas rules](https://linear.app/unnamed-system/issue/SDK-473/define-evidence-uncertainty-and-conflict-handling-for-atlas-rules): evidence scope, shared support, uncertainty, and requalification.
- [Choose the first useful Atlas coverage and consumer guarantees](https://linear.app/unnamed-system/issue/SDK-472/choose-the-first-useful-atlas-coverage-and-consumer-guarantees): first tradition/category obligations.
- [Validate reusable reference observations through PDX Native](https://linear.app/unnamed-system/issue/SDK-482/validate-reusable-reference-observations-through-pdx-native) and [Verify native observations before registration and parsing](https://linear.app/unnamed-system/issue/SDK-483/verify-native-observations-before-registration-and-parsing): accepted observation-contract refinements.
- [Define the offline snapshot and consumer contract](https://linear.app/unnamed-system/issue/SDK-477/define-the-offline-snapshot-and-consumer-contract) and [Decide Atlas repository, packaging, and release ownership](https://linear.app/unnamed-system/issue/SDK-479/decide-atlas-repository-packaging-and-release-ownership): consumer separation, exact source pins, retention, and publication authority.
- [Native evidence index](../native-evidence.md), [target records](../native/targets.md), [retrieval instructions](../native/retrieval.md), and [verification record](../native/verification.md): local experiment locations, qualification limits, and replay prerequisites.

The supplied task requests local Markdown, so this document is stored in Native rather than published as a new issue. The map and resolution comments remain the accepted decision record. Concrete public-contract details above synthesize those decisions for implementation review; they do not retroactively claim that every production design choice was settled by a prototype.
