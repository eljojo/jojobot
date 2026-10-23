use super::*;
use std::cell::RefCell;
use tantivy::collector::{Count, TopDocs};
use tantivy::postings::Postings;
use tantivy::query::AllQuery;
use tantivy::{DocAddress, DocSet, SegmentReader};

type Score = relevance::Rank;
type RankedMatch = (Score, DocAddress, Payload);

impl FullTextIndex {
    /// Rank and count in the same index walk; only the bounded winners load payloads.
    fn collect(
        &self,
        clauses: Vec<(Occur, Box<dyn Query>)>,
        limit: usize,
        asked: &SearchQuery,
        collapse_sessions: bool,
    ) -> Result<(Vec<RankedMatch>, usize), MemoryError> {
        let searcher = self.reader.searcher();
        // No clauses at all means "everything that matches the filters", and the
        // filters are the clauses — so this only happens for a bare kind-less
        // query, which validation refuses. AllQuery keeps it total anyway.
        let query: Box<dyn Query> = if clauses.is_empty() {
            Box::new(AllQuery)
        } else {
            Box::new(BooleanQuery::new(clauses))
        };
        let terms: Vec<_> = asked
            .terms()
            .map(|text| self.terms_of(text))
            .unwrap_or_default()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let positions_needed = terms.len() > 1;
        let mut segments = std::collections::HashMap::new();
        for segment in searcher.segment_readers() {
            let mut postings = Vec::new();
            let fields = std::iter::once(self.fields.text)
                .chain(asked.include_history.then_some(self.fields.history_text));
            for (field_at, field) in fields.enumerate() {
                let index = segment.inverted_index(field).map_err(store_err)?;
                for (term_at, term) in terms.iter().enumerate() {
                    let posting = index
                        .read_postings(
                            &Term::from_field_text(field, term),
                            if positions_needed {
                                IndexRecordOption::WithFreqsAndPositions
                            } else {
                                IndexRecordOption::Basic
                            },
                        )
                        .map_err(store_err)?;
                    postings.push((term_at, (field_at as u64) << 40, posting));
                }
            }
            let priority = segment
                .fast_fields()
                .u64("rank_priority")
                .map_err(store_err)?
                .first_or_default_col(relevance::CURRENT);
            let day_field = match asked.rank_clock {
                RankClock::RecordedAt => "rank_recorded",
                RankClock::HappenedAt => "rank_happened",
            };
            let day = segment
                .fast_fields()
                .u64(day_field)
                .map_err(store_err)?
                .first_or_default_col(0);
            segments.insert(segment.segment_id(), (postings, priority, day));
        }
        let score = move |segment: &SegmentReader| {
            let (postings, priority, day) = segments
                .get(&segment.segment_id())
                .expect("prepared search segment")
                .clone();
            let postings = RefCell::new(postings);
            move |doc, score| {
                let mut locations = Vec::new();
                for (term, field_offset, posting) in postings.borrow_mut().iter_mut() {
                    if let Some(posting) = posting
                        && posting.doc() <= doc
                        && posting.seek(doc) == doc
                    {
                        if positions_needed {
                            let mut offsets = Vec::new();
                            posting.positions(&mut offsets);
                            locations.extend(
                                offsets
                                    .into_iter()
                                    .map(|at| (*term, *field_offset + u64::from(at))),
                            );
                        } else {
                            locations.push((*term, 0));
                        }
                    }
                }
                locations.sort_unstable_by_key(|(_, at)| *at);
                relevance::rank(priority.get_val(doc), &locations, score, day.get_val(doc))
            }
        };
        let (top, total) = if collapse_sessions && asked.asked_by.is_some() {
            searcher
                .search(
                    &query,
                    &distinct::Page {
                        limit,
                        score,
                        analyzer: self.index.tokenizers().get(ANALYZER),
                        terms: asked
                            .terms()
                            .map(|text| self.terms_of(text))
                            .unwrap_or_default(),
                    },
                )
                .map_err(store_err)?
        } else {
            searcher
                .search(
                    &query,
                    &(TopDocs::with_limit(limit).tweak_score(score), Count),
                )
                .map_err(store_err)?
        };

        let mut out = Vec::with_capacity(top.len());
        for (score, address) in top {
            let document: TantivyDocument = searcher.doc(address).map_err(store_err)?;
            let raw = document
                .get_first(self.fields.payload)
                .and_then(|v| v.as_str())
                .ok_or_else(|| MemoryError::Store("indexed document lost its payload".into()))?;
            let payload: Payload = serde_json::from_str(raw).map_err(|e| {
                // **A parse that failed because this process loaded no kinds
                // is not a damaged index** (rule 68). Every stored record
                // carries a handle, so an empty set fails all of them at once,
                // and reporting that as the store breaking sends a caller to a
                // person for a fault a boot repairs.
                if kinds::all().is_empty() {
                    MemoryError::KindsNeverLoaded { attempted: None }
                } else {
                    MemoryError::Store(format!("indexed payload: {e}"))
                }
            })?;
            out.push((score, address, payload));
        }
        Ok((out, total))
    }

    pub fn search(&self, query: &SearchQuery) -> Result<Vec<Hit>, MemoryError> {
        Ok(self.search_page(query)?.hits)
    }

    pub fn search_page(&self, query: &SearchQuery) -> Result<RankedPage, MemoryError> {
        query.validate()?;
        // **The strict type question reads every thing its clause selects.**
        // The clause selects on the keys a thing carries and cannot see what a
        // key holds, so a mistyped thing passes it and takes a place in the cut
        // before the whole-match filter drops it. A depth cut over that set can
        // be filled by things that do not fit, and the answer is empty while a
        // well-typed thing exists. What the clause selects is bounded by the
        // things carrying every key, so there is no cut to be had.
        // The exact count must also follow conformance, which the index cannot see.
        let depth = if query.fits_type.is_some() {
            (self.reader.searcher().num_docs() as usize).max(1)
        } else {
            candidate_depth(query.limit)
        };

        // **A type query has no fact half.** The question is which THINGS
        // carry the keys, and a row is not a thing: returning its rows beside
        // the answer would crowd the things it asked about out of the limit.
        let (mut scored, mut total) = if query.is_thing_scoped() {
            (Vec::new(), 0)
        } else {
            self.collect(self.fact_clauses(query), depth, query, false)?
        };
        if !query.is_fact_scoped() {
            let (other, count) = self.collect(self.other_clauses(query), depth, query, true)?;
            scored.extend(other);
            total += count;
        }

        let terms = query.terms().map(|t| self.terms_of(t)).unwrap_or_default();
        // Every hit resolves against the same mirror reading.
        let mirror = self.docs.read().expect("doc mirror poisoned");
        let mut ranked: Vec<(Score, DocAddress, Hit)> = scored
            .into_iter()
            .map(|(score, address, payload)| {
                (score, address, payload.into_hit(&terms, &mirror, self))
            })
            .collect();
        // Match TopDocs' ascending DocAddress tie so a larger continuation page
        // extends the same ranking. The reader reloads only when the index changes.
        ranked.sort_by(|a, b| relevance::compare(&b.0, &a.0).then_with(|| a.1.cmp(&b.1)));

        // **How each thing answers, written onto the hit.** The selecting was
        // done by the clause in `other_clauses`, over the whole corpus; this
        // says how, which is what the caller asked for and which only the
        // mirror can answer. Nothing is expected to be dropped here — a thing
        // reaching this point carries a declared key — and the filter stays a
        // filter so that one which somehow does not cannot arrive claiming an
        // answer it has not got.
        // **Selecting is one clause; which of the two questions was asked
        // decides what survives it.** The tolerant one keeps a thing carrying
        // some of the keys and says which it lacks; the strict one keeps only
        // the things with nothing lacking. Both say how the thing answered,
        // because a caller that asked the strict question still wants to see
        // what it got rather than a bare list.
        //
        // **Applied to every hit, whichever half it arrived on.** A pin is
        // selected by the text alone, so a type filter reaching the ranked half
        // alone is defeated by any caller who also types a name, and hands back
        // a thing that does not fit with no answer on it to say so.
        let mut governed = |hit: &mut Hit| match query.typed() {
            Some((declared, strictly)) => {
                answer_with(declared, hit, &mirror) && (!strictly || whole(hit))
            }
            None => true,
        };

        // **Pinned first, and filtered like anything else.** An exact naming
        // still leads the answer when it survives the question, and it carries
        // the same answer a ranked hit would, because the same step writes it.
        let mut hits = self.pinned(query, &mirror);
        hits.retain_mut(&mut governed);
        ranked.retain_mut(|(_, _, hit)| governed(hit));
        if query.fits_type.is_none() {
            total += self.pins_outside_the_clause(query, &hits)?;
        }

        // **Deduped by identity, never by the whole hit.** The same thing can
        // reach the answer twice, once pinned and once ranked, and what hangs
        // off it — the type answer — is written after the pin was built. A
        // comparison that included it called two copies of one thing two
        // things.
        let mut seen: Vec<(&'static str, String)> = hits.iter().map(identity).collect();
        for (_, _, hit) in ranked {
            let id = identity(&hit);
            if !seen.contains(&id) {
                seen.push(id);
                hits.push(hit);
            }
        }
        if query.fits_type.is_some() {
            total = hits.len();
        }
        hits.truncate(query.limit);
        Ok(RankedPage { hits, total })
    }
    /// A naming guard can pin a name the text clause did not match. Count that
    /// small set separately, without fetching the rest of the matching payloads.
    fn pins_outside_the_clause(
        &self,
        query: &SearchQuery,
        pins: &[Hit],
    ) -> Result<usize, MemoryError> {
        if pins.is_empty() {
            return Ok(0);
        }
        let docs: Vec<_> = pins
            .iter()
            .filter_map(|hit| match hit {
                Hit::Entity { doc_id, .. } => Some((
                    Occur::Should,
                    Box::new(TermQuery::new(
                        Term::from_field_text(self.fields.doc_id, doc_id),
                        IndexRecordOption::Basic,
                    )) as Box<dyn Query>,
                )),
                _ => None,
            })
            .collect();
        let mut clauses = self.other_clauses(query);
        clauses.push(self.must_term(self.fields.class, CLASS_ENTITY));
        clauses.push((Occur::Must, Box::new(BooleanQuery::new(docs))));
        let matched = self
            .reader
            .searcher()
            .search(&BooleanQuery::new(clauses), &Count)
            .map_err(store_err)?;
        Ok(pins.len().saturating_sub(matched))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jojobot_domain::memory::types::{Field as Key, ValueType};
    use jojobot_domain::memory::{Boot, testing::InMemoryMemory};

    #[test]
    fn strict_type_count_checks_every_key_present_candidate() {
        let _booted = InMemoryMemory::booted();
        let roster = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../jojobot-domain/tests/fixture_roster.rs"
        ));
        let ordinary = roster
            .split_once("const ORDINARY: &[&str] = &[")
            .unwrap()
            .1
            .split_once("];")
            .unwrap()
            .0;
        let handles: std::collections::BTreeSet<_> = ordinary
            .lines()
            .filter_map(|line| {
                let handle = line.trim().strip_prefix('"')?.split_once('"')?.0;
                handle.starts_with("person:").then_some(handle)
            })
            .collect();
        assert!(handles.len() > 100, "a large kind from the roster");
        let candidates = handles.len();
        let index = FullTextIndex::open().expect("index");
        for (at, handle) in handles.into_iter().enumerate() {
            index
                .ingest_doc(
                    &DocScan {
                        doc_id: handle.into(),
                        title: handle.into(),
                        prose: String::new(),
                        entity: Some(Entity {
                            id: EntityId(handle.into()),
                            kind: EntityKind::PERSON,
                            name: handle.into(),
                            aliases: Vec::new(),
                            folded_names: Vec::new(),
                            source: "user-named".into(),
                            crm: None,
                            parent: None,
                            boot: Boot::OnDemand,
                            merged_into: None,
                            badge: None,
                            archived: None,
                        }),
                        facts: Vec::new(),
                        fields: [(
                            "crates".into(),
                            if at % 2 == 0 { "1" } else { "many" }.into(),
                        )]
                        .into(),
                        owner: None,
                        unmapped: Vec::new(),
                    },
                    &Default::default(),
                )
                .expect("index fixture");
        }
        let query = SearchQuery {
            kind: Some(EntityKind::PERSON),
            fits_type: Some(DeclaredType::new(
                "counted",
                vec![Key::required("crates", ValueType::Number)],
            )),
            ..SearchQuery::default()
        };
        let began = std::time::Instant::now();
        let page = index.search_page(&query).expect("strict type search");
        let elapsed = began.elapsed();
        assert_eq!(page.total, candidates.div_ceil(2));
        assert_eq!(page.hits.len(), search::DEFAULT_LIMIT);
        assert!(page.hits.iter().all(whole));
        println!(
            "SEARCH_LEAN_STRICT_MEASURE kind=person candidates={candidates} valid={} served={} elapsed_us={}",
            page.total,
            page.hits.len(),
            elapsed.as_micros()
        );
    }
}
