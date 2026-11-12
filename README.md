# vocabmod-se

vocabmod-se links the codes used in Swedish health data, from diagnoses
and procedures to medicines, laboratory tests, and social services, to
the international terminologies that research and cross-border exchange
use: SNOMED CT, LOINC, ATC, RxNorm, and the standard concepts of the
OMOP common data model. It holds 23 Swedish code systems and 110,101
mappings as plain text, with a command-line tool that exports them as
OMOP vocabulary tables and FHIR ConceptMaps. It is meant for data
engineers loading Swedish data into OMOP, for researchers and agencies
who need to know what a Swedish code corresponds to elsewhere, and for
anyone mapping Swedish terminologies who wants a place to start.

The mappings cover 58,260 of the 71,146 Swedish codes in scope (81.9%),
counting a code as covered when a mapping leads to a standard OMOP
concept. Every mapping carries a confidence tier. On a held-out sample,
the 19,073 mappings in the high tier agreed with a blinded, rubric-based
review in 90% of cases (95% CI 78–96), so they are reasonable candidates
for lower priority in expert review; 45% of the mappings could not be
assessed. The blinded reviews were made by language-model agents working
to a written rubric. No domain expert and no agency has reviewed the
mappings: verify a mapping before relying on it, and do not use the
corpus for clinical decisions.

![Coverage and confidence of the vocabmod-se mappings, by system](assets/figures/dist/vocab-coverage-standard.svg)

vocabmod-se is an independent project. I built it part-time, with
extensive use of AI coding agents, and release it as open source. It is
not a proposed standard, and I make no commitment to long-term
maintenance, but issues, pull requests, and discussions from researchers
and professionals are welcome. The
[data descriptor](content/data-descriptor/dist/vocabmod-se-data-descriptor.pdf)
explains what the corpus holds, how the mappings were made and graded,
and where they fall short; its first page summarizes the whole.
Svensk sammanfattning: [README.sv.md](README.sv.md).

## Where to start

You can use the corpus without running any software. It is plain text
under `vocab/`, one directory per code system (see
[The code systems](#the-code-systems)), and any text search will find a
code. The terms for reusing each part are set out in the descriptor's
licensing chapter and in [LICENSE](LICENSE). To load the mappings into
an OMOP database or a FHIR terminology service, build the tool (see
[The companion CLI](#the-companion-cli)) and run `omop-export` or
`fhir-export`. To assess the mappings, read the
descriptor's chapters on coverage, grading, and limitations, and its
Table 2, which sets the mappings against the code systems European
exchange guidance names for each clinical data element.

## Where the codes come from

The codes come from Swedish public registers and national
classifications: E-hälsomyndigheten (ICD-10-SE, KVÅ, the Swedish ICF,
and KSI, which it has published since 1 June 2026), Socialstyrelsen
(SPKOD), TLV, SCB, WHO, Equalis, and NOMESCO. The laboratory catalogue
titles come from the GMLF research project, where the first mappings
were made. The national classifications are carried as codes and
hierarchies with this project's own mappings, without their designation
catalogues. Where a review judged a mapping, its rationale may quote the
single title it judged from the Swedish national classifications or
TLV's published lists, with the source named, under the Swedish
Copyright Act's quotation right; RETTS triage titles are not quoted.
The names the corpus carries as designations are SCB's register names
(CC0), NPU terms under their terms of use, this project's own English
renderings of the KVÅ codes, and the laboratory catalogue titles.
Licensed terminologies (SNOMED CT, LOINC, MedDRA, UCUM) are referenced
by code only. The terms for each source are in [LICENSE](LICENSE).

## Layout

```text
vocab/                                the published tree
vocab/module.kdl                      the one module root
vocab/<domain>/<system>/              one directory per code system
vocab/<domain>/<system>/definitions.kdl
                                      the system's one entry
vocab/_registry/                      shared records: agents, licenses,
                                      conventions, external declarations,
                                      census
vocab/_registry/omop/omop.kdl         OMOP constants registry
schema/                               the schema definition typing the tree
schema/examples/                      self-contained per-system example suites
assets/figures/                       figure configurations
assets/figures/dist/                  figure data and the README figure
pkg/rs/vocab-proc/                    Rust processor generated from the schema
tools/vocabmod-se/                    the companion CLI
tests/fixtures/                       declarative verification scenarios
tests/tools/vocabmod-se-fixture/      the black-box harness running them
content/data-descriptor/              the data descriptor (source + PDF)
pkg/typ/                              descriptor template and embedded fonts
LICENSE                               licensing, per source
LICENSES/                             full license texts
```

Corpus data lives under `vocab/` and nowhere else. Program output
(`assets/`, `imports/`, `target/`) is not committed, apart from the
coverage figure data and image under `assets/figures/dist/` and the
descriptor PDF, which a fresh render reproduces byte for byte.

Every document in the corpus has one of three roles:

- **module** — `module.kdl`, the single entry document. It imports the
  registry and every system's entry.
- **entry** — one per system: `definitions.kdl`, which declares the
  system, its releases, categories, and mapping collections and imports
  the system's own subtrees. Reading a system's entry resolves that
  system; reading the module resolves everything.
- **data** — everything else: concepts, hierarchy, designations,
  variants, and mappings, each file opening with its own `context`
  block that binds it to one system release, so every data file reads
  identically alone or through its import chain.

Data files are range files of at most 500 lines, split at natural
code-family boundaries. Directory names carry no semantics; the
`parent` links in the data are authoritative, and the layout mirrors
them.

## The code systems

| System                                        | Directory                 | Content                                                     |
| --------------------------------------------- | ------------------------- | ----------------------------------------------------------- |
| ICD_10 (WHO base codes)                       | `condition/ICD_10/`       | WHO ICD-10 base codes and hierarchy only                    |
| ICD_10_SE (ICD-10-SE extensions)              | `condition/ICD_10_SE/`    | Swedish national extension codes, `extends` on the WHO base |
| ICD_O_3 (ICD-O-3 morphology)                  | `condition/ICD_O_3/`      | morphology codes                                            |
| ATC_YCODE                                     | `drug/ATC_YCODE/`         | ATC Y-codes                                                 |
| TLV_FBA (TLV förbrukningsartiklar)            | `drug/TLV_FBA/`           | consumables, including the 2026 stoma list                  |
| TLV_VNR (TLV package numbers)                 | `drug/TLV_VNR/`           | package numbers                                             |
| TLV_VGR (TLV varugrupperingskoder)            | `drug/TLV_VGR/`           | product grouping codes                                      |
| TLV_VNR_DOSE_FORM                             | `drug/TLV_VNR_DOSE_FORM/` | dose forms                                                  |
| RXNORM (NLM drug index)                       | `drug/RXNORM/`            | code-only endpoints with an ATC mapping surface             |
| ICF                                           | `function/ICF/`           | WHO ICF codes                                               |
| GEO_SE                                        | `geo/GEO_SE/`             | counties and municipalities                                 |
| KKA (KVÅ surgical procedures, KKÅ)            | `intervention/KKA/`       | surgical procedure codes                                    |
| KMA (KVÅ medical procedures, KMÅ)             | `intervention/KMA/`       | medical procedure codes                                     |
| NCSP (NOMESCO surgical procedures)            | `intervention/NCSP/`      | NOMESCO procedures, hierarchy carried by code structure     |
| LAB_SE (hospital local laboratory catalogues) | `lab/LAB_SE/`             | local laboratory measurements, LOINC/NPU mapping surfaces   |
| NPU                                           | `lab/NPU/`                | laboratory property codes (Equalis)                         |
| UNIT_SE                                       | `lab/UNIT_SE/`            | Swedish laboratory units                                    |
| LISA (SCB register bridge)                    | `LISA/`                   | register bridge to GEO_SE, declared external                |
| LISA_EDUCATION                                | `demo/LISA_EDUCATION/`    | education levels (ISCED-aligned)                            |
| CIVIL_STATUS_SE                               | `demo/CIVIL_STATUS_SE/`   | civil-status value set                                      |
| SPKOD (Socialstyrelsen specialty codes)       | `provider/SPKOD/`         | specialty codes                                             |
| KSI                                           | `social/KSI/`             | social-service codes                                        |
| TRIAGE_SE (RETTS triage levels)               | `triage/TRIAGE_SE/`       | triage levels                                               |

Cross-system claims whose source and target are both external (the
SNOMED–EDQM dose-form mappings) live as a mappings tree under
`drug/SNOMED_CT-EDQM/`. Licensed and private systems (SNOMED CT, LOINC,
MedDRA, the OMOP concepts, the external public systems) are declared in
`_registry/external.kdl` — defined nowhere, referenced by code only.
`OMOP_CUSTOM` declares this project's own reserved OMOP custom-concept
id space (the 2.1-billion bands) beside it; the bands are declared as
`custom-id-range` records, and a committed binding falling outside
every declared band fails the export.

## Reading the data

A code from another system is written `SYSTEM:CODE` — a mapping may
carry `target="ICF:d6407"`. Within a system's own data, bare codes
refer to the system bound by the file's `context`. Swedish national
extensions are modeled as `extends` on the `system` node: an ICD-10-SE
extension code's `parent` may name a shared WHO code, spelled
`parent="ICD_10:A169"`.

An example data file (`condition/ICD_10_SE/concepts/A/A083A-A999.kdl`):

```kdl
// ICD-10-SE extension codes, chapter A, codes A083A-A999
context {
    system "ICD_10_SE" release="2026"
}
defaults {
    concepts {
        status "active"
        class "extension"
    }
}
concept "A083A" parent="ICD_10:A083" {
    validity origin=imported {
        from "1997-01-01"
    }
}
```

Concepts are schema-legal with or without `name` nodes; the tree does
not carry the national classifications' designation catalogues, so most
of its concepts have no name.

The schema (`schema/vocab.kdl`) is the executable definition of the
record vocabulary, and `schema/examples/` holds a worked example suite
per system.

## Grades on each mapping

Every mapping carries a `confidence` value under the
`vocabmod-se-confidence@1` scheme declared in `_registry/conventions.kdl`:
a high, medium, or low tier, or, under
`vocabmod-se-confidence-unassessable@1`, one of five named reasons why
no tier could be assigned. The descriptor's chapter 3 explains how the
tiers were fit and how well they agreed with blinded review. A smaller
set of mappings also carries the two fields of the older
`mapping-quality@1` rubric, derived from the mapping's own comment: an
`effect` (what the mapping omits or changes beyond its stated direction)
and a `quality` score (0–100). The rubric is deterministic, ranks stated
losses below verified bases, and gives no score where the comment states
nothing it recognizes; a missing score means _not assessed_, never
_wrong_. Four versioned translation tables in the same file normalize
source labels into the corpus's record vocabulary. Mapping collections
are published as `publisher="ludvig-alden"`, the author's agent record
in `_registry/agents.kdl`.

## The companion CLI

`tools/vocabmod-se` is a Rust binary that reads the committed corpus
through `pkg/rs/vocab-proc` — a processor generated from the committed
schema by [kdlith](https://github.com/ludvigalden/kdlith), the typed
KDL processing framework the schema is written for (the dependency is
pinned by revision in the workspace manifest). Build it with Cargo
from the repository root:

```text
cargo build --release --bin vocabmod-se
```

Every invocation reads the whole corpus (about 40 seconds at this
scale). Paths resolve against the repository root, taken from `--root`
or the working directory; the schema defaults to `schema/vocab.kdl`, and
`omop-export`'s OMOP constants registry to
`vocab/_registry/omop/omop.kdl`.

All verbs share one exit convention: **0** clean (search: results or a
clean no-match line), **1** findings — a read error, an unknown
`--system` or `--collection`, a missing local dataset, a census
mismatch, or export drift under `--check` — and **2** usage (an unknown
`--tables`, `--only`, or `--field` value, or an option a verb does not
take, is rejected before the corpus is read).

### `omop-export`

Writes the seven Athena-complementary OMOP CDM tables:
CONCEPT, CONCEPT_RELATIONSHIP, SOURCE_TO_CONCEPT_MAP, CONCEPT_SYNONYM,
CONCEPT_ANCESTOR, CONCEPT_CLASS, and RELATIONSHIP. Every emitted row is
this corpus's own — concept ids at or above 2,000,000,000 and an empty
`standard_concept` — exactly the rows Athena's own exports never carry,
so the files merge beside them.

```text
vocabmod-se [--root DIR] omop-export
    [--schema PATH] [--registry PATH]
    [--out DIR]                       # default assets/omop/
    [--tables a,b,...]                # default all seven; unknown -> exit 2
    [--system ID]... [--collection ID]...   # unknown -> exit 1
    [--require-assessed|--include-unassessed]  # default require; both -> exit 2
    [--manifest]                      # MANIFEST.csv sidecar with row counts
    [--check]                         # byte-compare, write nothing; drift -> exit 1
    [--imports DIR]...                # merge imported layers; the manifest
                                      #   records the merged-reading counts
```

### `fhir-export`

FHIR consumers exchange terminology maps as ConceptMap resources, so the
corpus's mapping collections are also written in that form. Writes one
FHIR R5 ConceptMap document (JSON) per mapping collection with at least
one selected mapping, `ConceptMap-<collection>.json`: one group per
source and target system, one element per source code, and one target
per asserted target code.

```text
vocabmod-se [--root DIR] fhir-export
    [--schema PATH]
    [--out DIR]                       # default assets/fhir/
    [--system ID]... [--collection ID]...   # unknown -> exit 1
    [--require-assessed|--include-unassessed]  # default require; both -> exit 2
    [--manifest]                      # MANIFEST.csv sidecar with per-document counts
    [--check]                         # byte-compare, write nothing; drift -> exit 1
    [--imports DIR]...                # merge imported layers; the manifest
                                      #   records the merged-reading counts
```

The documents use R5, whose ConceptMapRelationship codes read source to
target as the corpus's kinds do: `equivalent`,
`source_is_narrower_than_target`, `source_is_broader_than_target`,
`related_to`, and `not_related_to` become `equivalent`,
`source-is-narrower-than-target`, `source-is-broader-than-target`,
`related-to`, and `not-related-to`. R4's equivalence codes state the
same directions from the target's side (`wider`, `narrower`), which
inverts every directional kind on the way out. R4's `inexact` would
name `partial_overlap` exactly; R5 has no such code, so those targets
carry `related-to`, the parent code, with the corpus kind kept in the
`vocabmod-relationship` target property.

A target carries the mapping's own comment, its notes joined by a blank
line; a note the corpus marks as possibly quoting a licensed source is
never emitted. R5 requires a comment on `source-is-broader-than-target`
and `not-related-to` targets of an active map, so such a mapping without
one is skipped and counted (`comment-required`). Display text is carried
only where the corpus carries a designation for the code, chosen as
`omop-export` chooses `concept_name`. The licensed and external systems
are defined nowhere in the tree, so their codes never carry a display.

Identifiers are deterministic: the resource id is the collection id; the
url is `https://github.com/ludvigalden/vocabmod-se/fhir/ConceptMap/<id>`;
version, title, and publisher are the collection's own; status is
`active`. Group systems take a system's declared `canonical-uri`, else
the HL7-published URI (SNOMED CT, LOINC, UCUM, RxNorm, ATC, ICD-10,
ICD-10-CM, ICD-O-3, MedDRA), else
`https://github.com/ludvigalden/vocabmod-se/fhir/CodeSystem/<system>`.
These identifiers name the resources; they are not promised to resolve.
The OMOP id spaces keep their minted URIs, since their codes are OMOP
concept ids, not codes of the vocabulary they index. Groups, elements,
and targets are sorted, and two runs over the same tree are
byte-identical.

### `figures`

Computes the figure data configured under `assets/figures/` — one
deterministic CSV per figure plus `MANIFEST.csv`. No figure value comes
from a configuration: every number is computed from the tree. Numbers
that need a local usage dataset (not committed) are skipped with a
named missing-input row, and the run then exits 1 after writing. The
`target-standard` claim selector matches claims whose target resolves
in the local Athena release to an OMOP-standard concept, by id or
code, directly or through one valid `Maps to`; it needs
the local Athena release, skipped with the same named-note mechanism
when the release is absent.

```text
vocabmod-se [--root DIR] figures
    [--schema PATH]
    [--figures DIR]                   # default assets/figures/
    [--out DIR]                       # default assets/figures/dist
    [--only NAME]...                  # unknown -> exit 2
    [--athena DIR]                    # default <root>/temp/athena/x
```

### `search`

Case-insensitive substring search over names, synonyms, and codes; one
tab-separated line per hit (system, code, field, locale, matched
text, layer), sorted by system then code.

```text
vocabmod-se [--root DIR] search QUERY
    [--system ID]                     # unknown -> exit 1
    [--field names|synonyms|codes|all]  # default all; unknown -> exit 2
    [--limit N]                       # default 50
    [--imports DIR]...                # merge imported layers into the reading
```

### `index`

Writes the deterministic statistical index of the corpus — six sorted
TSV tables under the output directory — and cross-checks every count
the committed census (`vocab/_registry/census.kdl`) covers in the same
run: any divergence is reported as a finding. The census is therefore a
verified invariant of every index run, and a corpus change that alters
the counts fails the next run until the census is updated.

```text
vocabmod-se [--root DIR] index
    [--out DIR]                       # default assets/index/
    [--system ID]...                  # unknown -> exit 1
    [--imports DIR]...                # merge imported layers into the reading
```

Over a merged reading the committed census still governs only the
committed tree: the cross-check covers committed rows, and the imported
layer's content is described by its own per-import manifests.

### `import`

Renders an operator-local source (never copied into the repository)
into KDL fragments under the gitignored landing zone, one fragment tree
per source and release, with a `manifest.kdl` recording the source
digest, format, and counts. The committed corpus is never written:
imports land under `imports/` (or, with
`--allow-committed`, under the gitignored `local/` document root), and
any other in-repository target is refused.

```text
vocabmod-se [--root DIR] import <format> --source PATH-OR-ALIAS
    [--into DIR]                      # default imports/<source>/<release>
    --system ID                       # the system the fragments bind
    --release VERSION                 # the release the fragments bind
    [--manifest] [--dry-run] [--allow-committed]
```

Supported formats: `claml` (WHO ClaML XML: preferred-rubric labels),
`pdf-text` (pdftotext -layout chapter text: code + title lines), `tdl`
(`!columns`/`!row` TDL with l10n name parts), `csv` (delimited, a
code/kod/id column plus `name_<locale>` columns), and `rf2` (an
unzipped RF2 snapshot directory: active concepts, FSN and synonym
terms). `--source` spells a plain path or `$NAME/<rest>` against an
`--alias NAME=DIR` binding.

`search`, `index`, `omop-export`, and `fhir-export` merge imported
fragments beside the committed tree via `--imports` (repeatable,
global). Committed values win on conflict: an imported row identical to
its committed counterpart deduplicates, and one with differing content
is a finding. Search hits carry their layer in the trailing column;
export manifests record the merged-reading counts.

### Keeping the generated processor current

`pkg/rs/vocab-proc` is generated code and is not edited by hand. From
the repository root, with a kdlith checkout available:

```text
kdlith gen schema --crate vocab-proc --out pkg/rs/vocab-proc \
    --kdlith-workspace schema/vocab.kdl --check
```

reports drift (and, without `--check`, rewrites the crate).

## Verification

The tool is verified by a black-box fixture suite: declarative
scenarios under `tests/fixtures/`, run by the harness at
`tests/tools/vocabmod-se-fixture` against the built tool. Each
scenario's `scenario.json` carries ordered tool runs judged by exit
code and stream substrings, plus checks the harness itself judges over
the scratch tree (byte determinism, emitted file shape, OMOP
invariants reconciled against an independent line-scan census of the
committed tree). The tool under test never interprets an expectation.

```text
vocabmod-se-fixture --fx-suite tests/fixtures --fx-tool <tool binary>
```

Scenarios that read the whole corpus declare `heavy: true`; a default
run executes only the fast negative controls (well under a second),
and the full suite runs under `--fx-heavy` — a few minutes on an
8-core machine and roughly 1 GB of memory at this corpus size. Scenario
scratch lands under the system temporary directory (`vocabmod-se-fixture/`) unless
`--fx-scratch-root` names another location. `--fx-list` lists the
scenarios (22: 15 heavy, 7 negative controls). Exact row-count
expectations are census pins: their values come from independently
verified counts over the committed tree and are updated the way a
census is. The suite's conventions are stated in
[tests/fixtures/README.md](tests/fixtures/README.md).

## The data descriptor

`content/data-descriptor/main.typ` is the source of the data descriptor,
and `content/data-descriptor/dist/vocabmod-se-data-descriptor.pdf` its
rendered PDF. It covers what the corpus holds, how far the mappings
reach, how they were made and graded, how to use the corpus, licensing,
and limitations, and every number in it can be checked against the
committed census. Rendering it locally requires [Typst](https://typst.app)
and the committed fonts, run from the repository root:

```text
typst compile --root . --font-path pkg/typ/assets/fonts \
    --creation-timestamp 1794398400 content/data-descriptor/main.typ
```

## Known limitations

No domain expert has reviewed the mappings, 45% of them could not be
given a confidence tier, and six of the data elements that European
exchange guidance names have no mappings, or almost none; the
descriptor's limitations chapter sets these out. Three smaller points
concern particular systems:

- KVÅ codes carry this project's own English renderings and no Swedish
  publisher designation; the `GEO_SE` municipality names keep their official
  Swedish form (SCB publishes no English names).
- The targets of the `lab-se-npu` collection keep their legacy 10-digit
  NPU ids; no reliable derivation from a legacy id to its current code
  is available.
- The official EDQM–SNOMED CT dose-form map is distributed under
  SNOMED CT license terms and is therefore not merged; the tree's
  `snomed-ct-edqm` collection remains hand-curated.

## Licensing

The code (tools, tests, the generated processor, the schema, and the
templates) is Apache-2.0, and the author's own mappings, registries, and
texts are CC BY 4.0. Material from other sources keeps its own terms:
the national classifications and registries (WHO ICD-10, ICD-10-SE,
KVÅ, KSI, SPKOD, ICF, NOMESCO NCSP, RETTS, TLV) are carried as codes and
hierarchy without their designation catalogues, as described under
Where the codes come from; NPU designations are reproduced under the NPU
terms with attribution, SCB data is CC0, RxNorm names are U.S. public
domain with NLM acknowledgement, and Orphanet codes are reused under
CC BY 4.0 with the attribution LICENSE gives. SNOMED CT, LOINC, MedDRA,
and UCUM are referenced by code only. [LICENSE](LICENSE) states the
terms per source, with the full license texts in [LICENSES/](LICENSES)
and machine-readable records in `vocab/_registry/external.kdl`
and `agents.kdl`.

## Author and contact

Ludvig Aldén, <vocabmod-se@ludvigalden.com>. Issues, pull requests,
and discussions: see [CONTRIBUTING.md](CONTRIBUTING.md); security reports:
[SECURITY.md](SECURITY.md).

## Citing

Cite the commit you used rather than a branch: the commit identifies
the exact tree, and the census inside it fixes the counts.
[CITATION.cff](CITATION.cff) carries the citation metadata.
