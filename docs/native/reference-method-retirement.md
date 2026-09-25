# Retired reference initializer method

The SDK-482 Rust port was removed from the product build at milestone 2 because no
supported Native operation called it. Its exact M45-observe results are retained as
[small expected cases](reference-method-cases.json); the executable inputs and original
27 mutation controls remain in the preserved `typed-extraction` prototype bundle and
Git history. [Discovery](discovery.md#reusable-reference-seam-retired) gives the source revision and retrieval route.

| Input shape | Established result | Boundary |
| --- | --- | --- |
| Ship `PostInit`: typed map call, same-type null, and joined `random_existing_design` reader | Candidate `CShipSize`, with empty, missing, and found alternatives | The typed map callee's internals were not proved. |
| District `PostInit`: length-and-byte collection scan and joined `district_type` reader | Candidate `CDistrictType`, first match or typed null | Collection element type and loader were not proved. |
| Planet-class `PostInit`: getter wrapper around a typed call | Candidate `CPlanetClass`, null or call result | Hash-table callee internals and authored field join were not proved. |
| Army `PostInit`: conditional event-target traversal and indirect jump table | Unknown, with an unresolved conditional | The four qualified compiler shapes did not match. |
| Relic `PostInit`: unchanged district scan shape | Candidate `CRelic`, with no authored field join | This was successful method reuse, not proof of a full resolver. |

The two unresolved callee shapes above are separate failures. Neither says that the
working caller selection, typed-null check, reader join, or linear scan must be
rewritten. A future public reference operation should start from these cases and
qualify the missing callee and ownership relationships before claiming more.
