# SDK-542 architecture review verification

The 2026-09-26 review was checked against the implementation and the M45 population retained
under `.local/sdk-542`. The diagnostic labels below refer to the review rubric supplied with
PR #83. They are investigation prompts, not automatic requirements to refactor.

## Confirmed and repaired

- **D1, duplicate function bodies:** command dispatch now borrows declaration function bytes.
  It retains only named views; it no longer copies the entire body inventory to a second input.
  Token recovery and dispatch share the name, address and body validation for those views.
  Existing symbol and pointer maps still belong to each standalone method input. They are
  immutable projections with one source, not competing authorities. Sharing those maps across
  all field questions would require a broader change to input lifetimes; this repair does not
  claim to eliminate that cost.
- **D2, repeated rules:** one member-signature predicate serves dispatch, delegation and ordering.
  The binding's narrower checks remain: a persistent slot requires the unscoped signature,
  while command constructor discovery selects scoped roots. One constructor-summary operation
  now invalidates the remaining receiver span and installs bounded vtable points. Each caller
  still proves ownership with its own allocation rules. The older nested-collection evaluator
  also tracks inserted/read objects and is a different state model; it was not folded into this
  operation. Diagnostic source joins share the completed-interval predicate but retain their
  distinct explicit-owner and unique-source correlation rules. The worker retains the same
  validation-hook list that it installs.
- **D4, registry-shaped grammar:** command children now carry only fields, token paths and gaps.
  Their normalization uses that ledger directly. It does not manufacture registry completeness,
  persistent destinations, collection storage or field-use data.
- **D5, empty fixed keys:** no established keys now yields `Unresolved`, like empty family and
  ordering results. `Partial` requires established values. The method still does not claim
  complete grammar or prove that no numeric keys are accepted.
- **D7, duplicate declaration analysis:** binding analysis orchestration derives the inventory
  once and passes it to the executable reader and the public query. The executable reader no
  longer calls declaration analysis. Constructor enrichment does not change the registration
  inventory; it supports the later concrete receiver join.
- **D8, fabricated join:** normalization receives the kind/family established by analysis. It no
  longer creates a `ReaderJoin` with empty arguments just to classify a reader entry.
- **D9, migration:** the caller migration now explicitly names nested collection ID changes.
  An unresolved generic persistent destination has no identity. This latter defect was also
  caught by PR review and repaired before the architecture review.
- **Protocol and visibility:** diagnostic stage names have one protocol authority and generated
  Python names. The request's storage limitation is named `storage_unavailable`; parsing can
  proceed independently. The recipe module is private again and exports only the needed type.

## Findings that do not establish a defect

**D8/P0: identity does not establish value kind.** `CommandReader` proves constructor-installed
virtual `Read` and `ReadMember` targets. It does not prove what the `Read` override accepts.
Marking every such receiver `Block` would add an unsupported fact. Before these repairs, all
12 target controls already reported `Block` with the correct trigger/effect family. Across the
full population, 137 trigger commands and 463 effects had an established block kind; the other
959 triggers and 611 effects retained `Unknown`, including unresolved receivers. The documented
[reader contract](reader-kinds.md) explicitly permits a known identity with unknown kind.
Normalization now has a negative control for that distinction, and installed parity asserts
kind and family for all 12 controls. Population reports count kind separately from identity.

**D2/D3: the family checks answer different questions.** Shared reader-entry classification
identifies the broad value accepted by established helper signatures. Constructor joins refine
a generic persistent destination with its concrete reader. Conditional normalization additionally
accounts for unresolved and rejected paths, which are not all present in the reader-only join
list. Removing that last check would promote conditional facts. The three family tables also
have different subjects: accepted reader-entry values, dispatched child commands, and the
constructor-selected modifier reader. An outer reader's family is not its set of child families:
`random_list` is an effect reader whose outer numeric keys lead to a separate effect-child grammar.
Build slots, collection layout and exact modifier/dispatch anchors stay in binding recipes;
shared signature classification remains in the existing reader method.

**D5: absent recorded evidence and complete states.** A legacy recording without `family` remains
`Unknown`; deserializing it must not perform new analysis. `NotApplicable` is an established
non-block fact, not the default for missing evidence. `Known` and known absent numeric grammar
remain expressible in recorded answers and the consumer contract even though this bounded method
currently does not produce them. Removing those distinctions would prevent callers from testing
complete versus partial and established absence versus unresolved evidence.

**Trust boundaries:** binding selection, worker access checks and reducer validation are separate
obligations. The reducer must reject incoherent or misplaced observations even when the normal
worker would not emit them. Stage-window checks therefore remain on both sides. The protocol
owns the vocabulary; the producer does not decide whether the final answer is complete.

D6 and D10 identified no missing ordering rule or negative control. Grammar and field methods
continue to share the bounded token evaluator and token recovery; the new command result no
longer exposes unrelated registry state through that reuse.
