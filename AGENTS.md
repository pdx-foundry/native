# PDX Native

Native is a new library with no consumers and an unstable API. Agents may make necessary development
changes without routine approval. Preserve the untracked knowledge acquired by prototypes and probes.
For API changes, cleanup, or reservation recovery, follow the
[development policy](docs/development-policy.md).

Native is a simple engine API, not an evidence archive. Read the [simplification decision](docs/design/simplification.md)
before you add an operation. Do not add replay paths, evidence descriptors, artifact hashes, qualification
records, or Cargo features. Public names prefer clarity to brevity (`registry_fields`, not `fields`).

Before you work on game launch, cleanup, injection, engine calls, memory layouts or discovery, read the
[engine knowledge index](docs/engine-knowledge.md). Match the exact build before you reuse a finding.

Native owns the platform and build methods. Atlas owns extraction fixtures, rule conclusions and coverage.
The code and its tests are the authority for supported operations; the knowledge pages keep the experiments.
