//! Count and page the same session passages the answer can show.

use super::*;
use tantivy::collector::{Collector, SegmentCollector};
use tantivy::columnar::StrColumn;
use tantivy::tokenizer::TextAnalyzer;
use tantivy::{DocAddress, DocId, SegmentReader, TantivyError};

type Rank = relevance::Rank;
type Winner = (Rank, DocAddress, Option<String>);

pub(super) struct Page<F> {
    pub limit: usize,
    pub score: F,
    pub analyzer: Option<TextAnalyzer>,
    pub terms: Vec<String>,
}

#[derive(Default)]
pub(super) struct State {
    winners: Vec<Winner>,
    sessions: HashSet<String>,
    ordinary: usize,
}

impl State {
    fn keep(&mut self, winner: Winner, limit: usize) {
        if let Some(key) = &winner.2
            && let Some(at) = self
                .winners
                .iter()
                .position(|held| held.2.as_ref() == Some(key))
        {
            if order(&self.winners[at], &winner).is_le() {
                return;
            }
            self.winners.remove(at);
        }
        self.winners.push(winner);
        self.winners.sort_unstable_by(order);
        self.winners.truncate(limit);
    }
}

fn order(a: &Winner, b: &Winner) -> std::cmp::Ordering {
    relevance::compare(&b.0, &a.0).then_with(|| a.1.cmp(&b.1))
}

pub(super) struct Segment<G> {
    limit: usize,
    ordinal: u32,
    score: G,
    passages: Option<StrColumn>,
    analyzer: Option<TextAnalyzer>,
    terms: Vec<String>,
    state: State,
    error: Option<TantivyError>,
}

impl<F, G> Collector for Page<F>
where
    F: Fn(&SegmentReader) -> G + Send + Sync,
    G: Fn(DocId, f32) -> Rank + 'static,
{
    type Fruit = (Vec<(Rank, DocAddress)>, usize);
    type Child = Segment<G>;

    fn for_segment(&self, ordinal: u32, reader: &SegmentReader) -> tantivy::Result<Self::Child> {
        Ok(Segment {
            limit: self.limit,
            ordinal,
            score: (self.score)(reader),
            passages: reader.fast_fields().str("session_passage")?,
            analyzer: self.analyzer.clone(),
            terms: self.terms.clone(),
            state: State::default(),
            error: None,
        })
    }

    fn requires_scoring(&self) -> bool {
        true
    }

    fn merge_fruits(&self, fruits: Vec<tantivy::Result<State>>) -> tantivy::Result<Self::Fruit> {
        let mut combined = State::default();
        for fruit in fruits {
            let state = fruit?;
            combined.sessions.extend(state.sessions);
            combined.ordinary += state.ordinary;
            for winner in state.winners {
                combined.keep(winner, self.limit);
            }
        }
        let total = combined.ordinary + combined.sessions.len();
        Ok((
            combined
                .winners
                .into_iter()
                .map(|(rank, address, _)| (rank, address))
                .collect(),
            total,
        ))
    }
}

impl<G: Fn(DocId, f32) -> Rank + 'static> Segment<G> {
    fn session_key(&mut self, doc: DocId) -> tantivy::Result<String> {
        let passage = self.passages.as_ref().ok_or_else(|| {
            TantivyError::InvalidArgument("indexed session lost its passage column".into())
        })?;
        let ordinal = passage.term_ords(doc).next().ok_or_else(|| {
            TantivyError::InvalidArgument("indexed session lost its passage".into())
        })?;
        let mut raw = String::new();
        if !passage.ord_to_str(ordinal, &mut raw)? {
            return Err(TantivyError::InvalidArgument(
                "indexed session lost its passage value".into(),
            ));
        }
        let (bot, session, text): (String, String, String) = serde_json::from_str(&raw)
            .map_err(|e| TantivyError::InvalidArgument(format!("indexed session passage: {e}")))?;
        let snippet = passages::snippet_with(self.analyzer.as_mut(), &text, &self.terms);
        Ok(format!("{bot}/{session}/{snippet}"))
    }
}

impl<G: Fn(DocId, f32) -> Rank + 'static> SegmentCollector for Segment<G> {
    type Fruit = tantivy::Result<State>;

    fn collect(&mut self, doc: DocId, score: f32) {
        if self.error.is_some() {
            return;
        }
        let rank = (self.score)(doc, score);
        let key = if rank.0 == relevance::SESSION {
            match self.session_key(doc) {
                Ok(key) => {
                    self.state.sessions.insert(key.clone());
                    Some(key)
                }
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            }
        } else {
            self.state.ordinary += 1;
            None
        };
        // Only distinct keys are unbounded for the exact count. Ranked payload
        // addresses stay bounded, even when every beat shows the same passage.
        self.state
            .keep((rank, DocAddress::new(self.ordinal, doc), key), self.limit);
    }

    fn harvest(self) -> Self::Fruit {
        match self.error {
            Some(error) => Err(error),
            None => Ok(self.state),
        }
    }
}

impl FullTextIndex {
    /// Write one beat of a session's chronology.
    ///
    /// **One document per entry, not per run.** A run is a list of beats a
    /// month apart; indexing it whole would make a hit point at the run and
    /// leave a reader to find the line, and would put a snippet around text
    /// that is mostly not what matched.
    pub(super) fn write_session_entry(
        &self,
        writer: &IndexWriter,
        session: &jojobot_domain::session::Session,
        entry: &jojobot_domain::session::JournalEntry,
    ) -> Result<(), MemoryError> {
        if self.should_fail_this_write() {
            return Err(store_err("test fault: add_document"));
        }
        let f = &self.fields;
        writer
            .add_document(doc!(
                f.class => CLASS_SESSION,
                f.rank_priority => relevance::SESSION,
                f.text => format!("{} {}", entry.text, session.focus),
                f.session_passage => serde_json::to_string(&(session.bot.to_string(), &session.id.0, &entry.text)).map_err(store_err)?,
                f.session_id => session.id.0.as_str(),
                f.entry_id => entry.id.0.as_str(),
                f.owner => session.bot.to_string(),
                f.payload => payload_json(&Payload::Session {
                    session: session.id.clone(),
                    bot: session.bot.clone(),
                    working_on: Some(session.focus.clone()).filter(|f| !f.is_empty()),
                    text: entry.text.clone(),
                })?,
            ))
            .map_err(store_err)?;
        self.session_entries_written
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        Ok(())
    }
}
