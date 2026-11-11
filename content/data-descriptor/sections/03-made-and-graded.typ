#import "/pkg/typ/common/lib.typ": data-table
// Made and graded — how mappings entered the corpus, and what the
// confidence tiers and the older quality score say about them.

= 3. How the mappings were made and graded

The mappings come from three sources, and they differ in how much evidence stands behind each one. That is why every mapping is graded.

The oldest mappings were imported: publisher crosswalks and register material, and the early mappings from GMLF, made there for an initial assessment of the project's laboratory data from six hospitals and of how that data could be mapped to OMOP. A program converted this material into the published form. It is deterministic, so repeated runs over the same input give the same tree byte for byte, and a source row it cannot interpret stops the run rather than being dropped. Labels from the sources were normalized through four translation tables committed with the registry, and the six hospital catalogues were merged under a published rule into 1,320 concepts, with 54 local spellings kept as variants. Because its sources cannot be redistributed, the program is not published: a reader can re-validate the tree and re-derive every statistic, but not re-run the import.

The second source is composition from what the corpus already holds: 311 county and municipality codes bound to their OMOP concept ids, 1,170 KSI codes mapped onward through ICF mappings that end at standard concepts, and 451 ICD-10-SE codes mapped to ICD-10-CM parents by shortening the code, which caps them at medium confidence.

The third source is a review of every code that was still uncovered, and it added 9,889 mappings. For each code, BioLORD-2023-M, a multilingual biomedical language model, proposed candidate concepts from OMOP's standard vocabularies; on a held-out sample the accepted concept was among its top ten for 59.0% of the codes where one existed. Language-model agents then judged the candidates against a written rubric without seeing the model's ranking, in two independent blind passes. 8,987 mappings were committed because both passes chose the same concept. For 845 codes the two passes chose different concepts that were both defensible; a third blind pass, shown both without knowing which pass chose which, picked one for 788 codes and kept both as alternatives for 57, adding 902 mappings. Two such passes agree about 81% of the time, which is the ceiling on how precisely this kind of review can be measured.

Every mapping carries a confidence tier, or one of five named reasons why it cannot be graded. A tier is decided by three facts: whether the target is OMOP-standard, where BioLORD ranks the target among its candidates, and how high it scores. A standard target ranked first above a fixed score is high, ranked first below it or second or third is medium, and ranked lower is low. The rule was fit on 588 reviewed cases and tested on a held-out sample. All 9,889 review mappings are high because blind review accepted them, the 902 tie-break mappings included; the held-out agreement below was measured on the rule, not on them.

#data-table(
  columns: (1fr, auto, auto, 2.4fr),
  header: ([Tier], [Mappings], [Share], [Held-out agreement with blinded review]),
  rows: (
    ([High], [19,073], [17%], [90% (95% CI 78–96)]),
    ([Medium], [8,837], [8%], [71.1% (95% CI 55–83)]),
    ([Low], [32,996], [30%], [5.6%]),
    ([Unassessable], [49,195], [45%], [—]),
  ),
)

The high tier agreed with the blinded, rubric-based review in 90% of cases (95% CI 78–96) on a held-out sample, so its mappings are reasonable candidates for lower priority in expert review. A low tier means that nothing independent corroborates the target, not that the target is wrong. Most unassessable mappings have a target outside OMOP's vocabularies or a source that is not a Swedish code, as in the RxNorm-to-ATC crosswalks. Precision above the roughly 81% two-pass ceiling cannot be told apart from reviewer disagreement, and no domain expert has reviewed the released mappings.

An older and narrower grade sits beside the tiers. A published rubric reads the comment that records each mapping's basis and derives a quality score, and an effect where the comment states that the mapping loses part of the source's meaning. Stated losses rank first, so a mapping that admits a loss never gets a full score. 3,311 mappings (3.0%) carry a score: 1,937 at 100 (a verified or exact basis), 679 at 60 (a step up the hierarchy), and 695 at 40 (a stated approximation or lost detail). A mapping without a score has not been assessed by this rubric, which says nothing about whether it is wrong.
