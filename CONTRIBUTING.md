# Contributing

Issues, pull requests, and discussions are welcome, from researchers and
professionals in particular. The project is maintained on a best-effort
basis, without a commitment to long-term maintenance, so a reply may
take time. You can also write to <vocabmod-se@ludvigalden.com>.

A correction to a mapping states the mapping id (for example
`A083A.1`), what is wrong with it, and the source that shows it: a
register release, a classification edition, or a terminology browser
entry. The mapping's `comment` is its basis, so a corrected mapping
carries a corrected comment.

## Identifiers and names

Each system has one identifier and one name. The identifier is the
official short name, upper-cased, with spaces, hyphens, and dots
replaced by underscores (`ICD_10_SE`, `SNOMED_CT`); directories naming
a system use it, and collection ids use its kebab form
(`icd-10-se-snomed-ct`). In KDL values and paths the identifier is
written out; prose uses the name (`ICD-10-SE`, `SNOMED CT`), and a
sentence never mixes the two forms.

Before a change is committed, these checks pass locally:

```sh
cargo build --release --locked --workspace
target/release/vocabmod-se-fixture --fx-suite tests/fixtures \
    --fx-tool target/release/vocabmod-se
```

The kdlith dependency is fetched from its public GitHub repository.
A change to the data descriptor is re-rendered with Typst (the command
is in the README) and compared byte for byte with the committed PDF.
The full fixture suite (`--fx-heavy`; see
[Verification](README.md#verification) in the README) covers the tool's
behavior against the whole corpus.

By contributing you agree that your contribution is licensed under the
terms in [LICENSE](LICENSE): Apache-2.0 for code, CC BY 4.0 for data
and text.
