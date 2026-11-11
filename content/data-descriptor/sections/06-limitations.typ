// Limitations — what a user should know before relying on the
// mappings, in the order the earlier chapters set up.

= 6. Limitations

The largest limitation is that no domain expert has reviewed the mappings. The confidence tiers of chapter 3 grade the evidence behind each mapping, 45% of the mappings could not be graded at all, and only 3,311 (3.0%) carry the rubric's quality score. Before a mapping is relied on, it should be checked against its comment and, for any consequential use, by someone qualified to judge it.

The corpus reflects the releases it imported. KVÅ, ICD-10-SE, and the TLV registers are snapshots, so a code added after its snapshot is missing. Designations limit retrieval too: the candidate search works from a code's name, so a code without one has nothing to search with, and 2,147 mappings are unassessable for that reason.

Six data elements of Table 2 lie outside the corpus, in the table's order. Medication reaches ATC classes and dose forms but no active substances (EMA SMS) and no route of administration. Immunization reaches only three ATC J07 packages, since most vaccines sit outside the TLV benefit registers. Allergies, pregnancy, and document types have no collection; closing them needs a Swedish source system the corpus does not carry.

Two smaller points concern particular records. The 103 laboratory mappings that keep their legacy ten-digit NPU identifiers have no reliable route to the current spelling, so a user resolving them against a current NPU release has to translate them. And the project's reserved OMOP custom-concept ranges are a convention for sharing between users of this corpus, never OHDSI content.

The corpus is meant for looking up what Swedish health-data codes correspond to in reference terminologies, for seeing where a mapping came from and what it claims, and for building OMOP vocabularies to use beside Athena, reproducibly from a pinned commit. It is not a terminology server, and it is neither clinical guidance nor a basis for clinical decisions. Nor is it complete: further work would extend coverage and put the confidence grades on firmer ground, above all through expert review.
