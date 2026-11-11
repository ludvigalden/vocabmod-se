#import "/pkg/typ/common/lib.typ": data-table, wide, coverage-data, coverage-figure, coverage-caption
// Coverage — the target-standard rule, the per-system figure, and the
// finding about codes without a suitable target. Production and
// grading are chapter 3.

= 2. How far the mappings reach

Most uses of the mappings need a Swedish code to end at a concept the OMOP common data model treats as standard, because analyses and exports work with those concepts. A code therefore counts as covered only when a mapping or identifier binding leads to an OMOP-standard concept, directly or through one "Maps to" step. By that rule the mappings cover 58,260 of the 71,146 Swedish codes in scope, 81.9%; the figure shows the share per system and the confidence tier (chapter 3) of the covering mappings.

#let coverage = coverage-data("/assets/figures/dist/vocab-coverage-standard.csv")
#wide(coverage-figure(coverage))

#coverage-caption(coverage)

Two registers hold most of the codes the rule does not reach. TLV's register of consumables and medical aids leaves 3,328 of its 4,339 codes uncovered, mostly products and devices for which no standard vocabulary offers a target. NPU leaves 5,606 of its 9,878 laboratory codes uncovered, among them blood-group antibody concentrations, allergen-specific IgE, and HLA typing, which no standard vocabulary names. These gaps have been examined, not only counted. The review described in chapter 3 worked through all 22,682 codes that were uncovered before it and covered 9,796 of them. Of the 12,886 that remain uncovered, reviewers found no suitable standard concept among the candidates for 10,625; for 2,174 the two blind review passes chose different targets, 75 lacked the information needed to judge, and 12 have no name to search with.

Dose forms lie outside the rule. TLV's 240 dose forms are mapped to EDQM standard terms as attributes of a package, not as codes to be covered, so they are left out of the 71,146.
