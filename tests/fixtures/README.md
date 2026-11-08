# Fixture suite

Declarative black-box scenarios for the vocabmod-se CLI, run by the
harness at `tests/tools/vocabmod-se-fixture` (its `--fx-*` options
document the interface). Each scenario directory carries a strict
`scenario.json` manifest: ordered `steps` (tool runs judged by exit code
and stream substrings, plus deliberate corruptions) and `checks` the
harness itself judges over the scratch tree (byte determinism, emitted
file shape, OMOP Athena-complementary invariants with an independent
line-scan census of the committed tree, manifest and row-count
reconciliation). The tool under test never interprets an expectation.

Scenarios that read the whole corpus (a ~40 s reading per invocation)
declare `heavy: true`: a default run executes only the fast negative
controls, and the full suite runs under `--fx-heavy`. The suite keeps
exactly one full-table OMOP scenario (`omop/determinism-full`); every
other OMOP scenario scopes with `--tables`/`--system` to bound runtime.

Some expectations pin exact row counts (the assessed-gate tables, the
`TRIAGE_SE` concept count). These are census pins: their expected values
come from independently verified counts over the committed tree, and a
deliberate corpus change updates them the way a census does.
