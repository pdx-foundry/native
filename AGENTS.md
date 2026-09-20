# PDX Native

Native is a new library with no consumers and an unstable API. Agents may make necessary development
changes without routine approval. Preserve the untracked knowledge acquired by prototypes and probes.
For API changes, cleanup, reservation recovery, or qualification/promotion, follow
[development policy](docs/development-policy.md). It supersedes older approval and blanket evidence-retention rules.

Native is a simple engine API, not an evidence archive. Read the [simplification decision](docs/design/simplification.md)
before you add an operation. Do not add replay paths, evidence descriptors, artifact hashes, qualification
records, or Cargo features. Public names prefer clarity to brevity (`registry_fields`, not `fields`).

For game launch, cleanup, injection, engine calls, memory layouts, discovery, or target qualification, read [docs/native-evidence.md](docs/native-evidence.md) first. Follow its capability page and verify the cited bundle before reusing a finding.

Native owns platform/build operations and native-method qualification. Atlas owns extraction fixtures, rule conclusions, and coverage. Implemented adapters and their qualification tests will become the authority for supported operations; these records preserve bounded experiments.
