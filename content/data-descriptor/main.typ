// vocabmod-se data descriptor.
//
// Compile from the repository root:
//   typst compile --root . --font-path pkg/typ/assets/fonts --creation-timestamp 1794398400 \
//     content/data-descriptor/main.typ content/data-descriptor/dist/vocabmod-se-data-descriptor.pdf
//
// The pinned creation timestamp keeps consecutive renders byte-identical.
// Every number in this report is cited from a source of record: the
// committed repository tree, its README and LICENSE, the committed
// census, or the project's own evaluation program.
#import "/pkg/typ/common/lib.typ": descriptor-doc

#show: descriptor-doc.with((
  title: "Data descriptor",
  subtitle: "vocabmod-se — a structured resource for Swedish health-data terminologies and cross-system mappings",
  author: "Ludvig Aldén",
  accent: rgb("#17506d"),
  paper: rgb("#FFFFFF"),
  tint: rgb("#eef2f5"),
  meta: (
    ("Author", "Ludvig Aldén"),
    ("Version", "1.0"),
    ("Date", "2026-11-11"),
    ("Status", "Self-published technical report; not peer reviewed"),
    ("Describes", "the vocabmod-se corpus, release 1.0"),
    ("Repository", link("https://github.com/ludvigalden/vocabmod-se")[github.com/ludvigalden/vocabmod-se]),
    ("Contact", link("mailto:vocabmod-se@ludvigalden.com")[vocabmod-se\@ludvigalden.com]),
  ),
))

#include "sections/00-in-brief.typ"
#include "sections/01-the-resource.typ"
#include "sections/02-coverage.typ"
#include "sections/03-made-and-graded.typ"
#include "sections/04-using-the-corpus.typ"
#include "sections/05-licensing-and-publication.typ"
#include "sections/06-limitations.typ"
