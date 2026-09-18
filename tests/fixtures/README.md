# Replay fixtures

`synthetic/` is newly authored test data for the retained SDK-483 record format. It contains no
game capture or production qualification. Descriptors mark it `synthetic`; the public result keeps
that origin even when the bounded synthetic window is complete. Cases cover normal, missing hook,
dropped record, and observation-worker loss. Shared artifacts avoid copying common provenance.

`private/` contains only hash/size-pinned portable references to the four accepted historical
SDK-483 attempts. Pins were derived from the verified `typed-extraction` file manifest recorded
in `docs/native/source-inventory.json`. No raw private observations, sources, or fixtures are tracked
here. Prepare a separate working root with `tools/prepare-private-replay.py`; the test reports this
prerequisite explicitly on a clean checkout. Descriptor identity covers every reference and survives
relocation. Restored originals and sealed archives remain unchanged.

The game rewrote the profile settings in three completed attempts. Their pre-launch settings bytes
are absent; replay verifies the post-run archive identity and reports the original input gap separately
from bounded observation completion. Other source/fixture/content-manifest identity mismatches fail.

The immutable descriptor has format `pdx-native/sdk-483-replay-v1` and observation contract
`pdx-native/early-read-entries-v1`. A reference stores `path`, `sha256`, and `bytes`. The caller pins
the descriptor reference; the descriptor pins the core manifest/request/trace/owner and supporting
artifacts. Hashes prove byte identity, not authenticity, native qualification, or rule conclusions.
Only a trusted descriptor pin identifies the intended historical evidence.
