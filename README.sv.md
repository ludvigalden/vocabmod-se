# vocabmod-se – svensk sammanfattning

vocabmod-se kopplar koderna i svenska hälsodata till de internationella
terminologier som forskning och informationsutbyte över gränser
använder, som SNOMED CT, LOINC, ATC, RxNorm och standardbegreppen i
datamodellen OMOP. Samlingen omfattar 23 svenska kodverk med 89 497
begrepp, bland dem ICD-10-SE, KVÅ (KKÅ och KMÅ), svenska ICF, KSI,
Socialstyrelsens specialitetskoder, NPU, TLV:s varunummer och lokala laboratoriekataloger från sex
sjukhus, och 110 101 mappningar från dem. Allt ligger som vanlig text
som kontrolleras mot ett gemensamt schema, och ett kommandoradsverktyg
exporterar mappningarna som OMOP-vokabulärtabeller och FHIR
ConceptMaps.

Mappningarna täcker 58 260 av de 71 146 svenska koder som ingår
(81,9 procent), räknat som koder med en mappning som leder till ett
standardbegrepp i OMOP. Varje mappning har en konfidensnivå. De 19 073
mappningarna på den högsta nivån stämde i 90 procent av fallen
(95 % KI 78–96) med en blindad granskning enligt en skriftlig
bedömningsmall i ett separat testurval, och de bör därför kunna
prioriteras lägre vid expertgranskning. 45 procent av mappningarna
kunde inte bedömas. De blindade granskningarna gjordes av
språkmodellsagenter som arbetade efter bedömningsmallen. Ingen
sakkunnig och ingen myndighet har granskat mappningarna, så kontrollera
en mappning innan du förlitar dig på den. Samlingen ska inte användas
för kliniska beslut.

Jämfört med de kodverk som eHealth Networks riktlinjer och HL7 Europes
implementeringsguider anger per dataelement (de är inte bindande) når
samlingen diagnoser (31 420 mappningar), sällsynta sjukdomar (230),
åtgärder (8 051), laboratorieresultat (6 371), enheter (14),
läkemedelsklass enligt ATC (37 945), beredningsform (514) och
funktionstillstånd via ICF (3 086). Den saknar substansidentiteter
(EMA SMS), administreringsväg, allergier, graviditetshistorik och
dokumenttyper, och vacciner nås bara genom tre förpackningar. Hela
tabellen finns i kapitel 1 i databeskrivningen.

De nationella klassifikationerna, som E-hälsomyndigheten har gett ut
sedan den 1 juni 2026 (specialitetskoderna ges ut av Socialstyrelsen), finns med som
koder och hierarki men utan sina förteckningar över benämningar. Där en
mappning har granskats kan motiveringen citera den enskilda benämning
som bedömdes ur de nationella klassifikationerna eller TLV:s
publicerade listor, med källan angiven, enligt upphovsrättslagens
citaträtt. Benämningar ur RETTS triage citeras inte.

vocabmod-se är ett fristående projekt. Jag har byggt det på deltid, med
omfattande hjälp av AI-baserade kodagenter, och publicerar det som öppen
källkod. Det är inget förslag till standard och jag utfäster mig inte
att underhålla det långsiktigt, men ärenden, pull requests och
diskussioner från forskare och yrkesverksamma är välkomna.
Databeskrivningen,
[content/data-descriptor/dist/vocabmod-se-data-descriptor.pdf](content/data-descriptor/dist/vocabmod-se-data-descriptor.pdf)
(på engelska), sammanfattar det mesta på första sidan, och
[README.md](README.md) beskriver samlingen och verktyget i detalj.
Samlingen ligger under `vocab/` och kan läsas utan att köra
något program.

Projektets egna data och mappningar är licensierade under CC BY 4.0 och
koden under Apache-2.0; villkoren för varje källa står i [LICENSE](LICENSE).

Ludvig Aldén, <vocabmod-se@ludvigalden.com>. Ärenden, pull requests och
diskussioner: se [CONTRIBUTING.md](CONTRIBUTING.md).
