#import "/pkg/typ/common/lib.typ": data-table, wide
// What the corpus holds — the systems, their designations, the
// mappings, and how the mappings meet the data elements European
// exchange guidance names. Coverage is chapter 2, production and
// grading chapter 3.

= 1. What the corpus holds

The corpus holds twenty-three Swedish code systems with 89,497 concepts, and the mappings that connect them to one another and to international terminologies. Table 1 groups the systems by clinical domain.

#wide(data-table(
  columns: (1.5fr, 2.2fr, 0.8fr),
  header: ([Domain], [Systems], [Concepts]),
  rows: (
    ([Diagnoses], [ICD-10 (WHO base), ICD-10-SE, ICD-O-3 morphology], [39,303]),
    ([Procedures], [KVÅ surgical and medical, NCSP], [15,650]),
    ([Pharmaceuticals], [TLV package numbers, product groups, consumables, dose forms; ATC Y-codes], [19,117]),
    ([Laboratory], [NPU, six hospitals' local catalogues, Swedish unit spellings], [11,213]),
    ([Function and social services], [ICF, KSI, SPKOD], [3,858]),
    ([*Total*], [], [*89,497*]),
  ),
))

The total includes 356 concepts in four small registers (counties and municipalities, civil status, education levels, and triage levels), which the table does not show as rows. Every count in this report can be checked against the committed census (`_registry/census.kdl`).

Most concepts carry a code but no name: 19,928 carry at least one designation, and the other 69,569 (77.7%) carry none. For the national classifications — ICD-10-SE, KVÅ (KKÅ and KMÅ), the Swedish ICF, and KSI, which E-hälsomyndigheten has published since 1 June 2026, SPKOD from Socialstyrelsen, and the rest — that is a publication boundary rather than a source gap: their publishers grant no license to redistribute the designation catalogues, so the tree holds codes, hierarchy, and validity. Where terms do permit carriage, the designations are there: NPU's property strings verbatim under the NPU terms of use, SCB's county and municipality names under CC0, and RxNorm product names by permission of NLM. The English KVÅ designations are this project's own renderings of the Swedish codes.

The mappings sit in versioned collections, each from one source system to one target system. Of the 110,101 mappings, 44,450 (40%) target SNOMED CT and 37,945 (34%) target ATC. Most state the Swedish code as narrower than its target (82.1%), as when a medicine package maps to the ATC class that contains it; 15.8% state equivalence, and the rest partial overlap, a looser relation, or a broader source. Counted apart are 8,337 identifier bindings, which give a Swedish code an OMOP concept id without claiming any relationship of meaning.

For a reader preparing Swedish data for European exchange, Table 2 sets the mappings, element by element, against the code systems the eHealth Network guidelines and HL7 Europe implementation guides name (non-binding): the Patient Summary Guideline (release 3.4), the ePrescription and eDispensation Guideline (release 3.1), and the HL7 Europe FHIR guides. Regulation (EU) 2025/327 names no code system itself; the coding of the European electronic health record exchange format is left to an implementing act due by March 2027, not adopted as of November 2026.

#wide(data-table(
  columns: (1.2fr, 1.35fr, 2.55fr, auto, auto),
  header: ([Data element], [Named systems], [Corpus collections], [Mappings], [Quality-scored]),
  rows: (
    ([Diagnoses], [ICD-10, SNOMED~CT], [ICD-10-SE → SNOMED CT, also as OMOP ids and via code truncation; ICD-O-3 → SNOMED CT], [31,420], [1,811]),
    ([Rare diseases], [Orphacode], [ICD-10-SE → Orphanet], [230], [0]),
    ([Procedures], [SNOMED~CT], [KVÅ surgical and medical → SNOMED CT (7 to LOINC and RxNorm)], [8,051], [805]),
    ([Laboratory results], [LOINC, NPU, SNOMED~CT], [hospital catalogues → LOINC, NPU; NPU → LOINC, SNOMED CT], [6,371], [552]),
    ([Units], [UCUM], [Swedish unit spellings → UCUM], [14], [0]),
    ([Medication class], [ATC], [TLV package numbers → ATC; RxNorm → ATC], [37,945], [0]),
    ([Active substance], [EMA SMS], [none], [0], [0]),
    ([Dose form], [EDQM], [TLV dose forms → EDQM; SNOMED CT → EDQM], [514], [0]),
    ([Route], [EDQM], [none], [0], [0]),
    ([Functional status], [ICF], [KSI → ICF; ICF → SNOMED CT, LOINC], [3,086], [112]),
    ([Vaccines], [SNOMED~CT, ATC], [three TLV packages → ATC J07], [3], [0]),
    ([Allergies], [SNOMED~CT, ATC], [none], [0], [0]),
    ([Pregnancy], [SNOMED~CT], [none], [0], [0]),
    ([Document types], [LOINC], [none], [0], [0]),
  ),
))

Census counts; an arrow reads source to target, and quality-scored counts the mappings that carry the rubric score of chapter 3, a narrower grade than the confidence tiers. ICD-10-SE extends ICD-10, so its codes are ICD-10 codes or children of one. The vaccine packages are counted in the medication-class row, and the 153 SNOMED CT mappings of ICD-10-SE chapter O (pregnancy, childbirth, and the puerperium) in the diagnosis row. Care plans are left out, because the guidance binds no code system to them. The mappings reach eight of the fourteen elements, and vaccines only through three packages. Quality scores are densest for procedures and laboratory results; the medication, dose-form, unit, and rare-disease rows carry none.
