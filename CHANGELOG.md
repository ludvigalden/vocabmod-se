# Changelog

## 1.0 - 2026-11-12

First publication. The corpus carries 23 Swedish code systems —
national classifications, registers, and laboratory terminologies — as
plain-text KDL documents typed by one committed schema, with 110,101
mappings onto international reference terminologies and OMOP concepts.
A review of every code still uncovered added 9,889 mappings, bringing
coverage to 58,260 of 71,146 in-scope codes (81.9%), and every mapping
carries a confidence tier or a named reason why none could be assigned.
The companion `vocabmod-se` CLI exports the seven Athena-complementary
OMOP CDM tables and one FHIR R5 ConceptMap per mapping collection,
writes the statistical index cross-checked against the committed
census, computes the figure data, and searches names, synonyms, and
codes. The black-box fixture suite under `tests/fixtures/` verifies the
tool against the corpus, and the data descriptor PDF re-renders byte
for byte from its source.
