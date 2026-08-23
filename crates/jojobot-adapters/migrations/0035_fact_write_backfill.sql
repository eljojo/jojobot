-- Every claim that predates the substrate becomes its own first write.
--
-- **A projection over an empty write set answers nothing**, so a claim with no
-- write behind it would go dark the moment anything reads the substrate rather
-- than the row. This is what stops that, and it runs before any read moves.
--
-- **History starts here.** No correction already made becomes recoverable: the
-- old words were overwritten by the writes that produced this state, and this
-- copies what stands rather than inventing what stood. That is correct rather
-- than a shortfall — a substrate that claimed to know what a claim said last
-- year would be making it up.
--
-- Ordinal one for every row, because each is the only write its claim has.
INSERT INTO fact_write (entity, fact_id, ordinal, content, details, provenance, standing,
                        status, date, edge_shape, edge_object, derived_from, derived_from_id,
                        inserted_at, stale_after)
SELECT f.entity, f.id, 1, f.content, f.details, f.provenance, f.standing,
       f.status, f.date, f.edge_shape, f.edge_object, f.derived_from, f.derived_from_id,
       f.inserted_at, f.stale_after
FROM fact f
WHERE NOT EXISTS (SELECT 1 FROM fact_write w WHERE w.entity = f.entity AND w.fact_id = f.id);
