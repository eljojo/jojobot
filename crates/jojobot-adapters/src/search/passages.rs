use super::*;
use jojobot_domain::memory::search::excerpt::{self, Match};

impl FullTextIndex {
    fn matches(&self, text: &str, terms: &[String]) -> Vec<Match> {
        self.index
            .tokenizers()
            .get(ANALYZER)
            .map(|mut analyzer| matches_with(&mut analyzer, text, terms))
            .unwrap_or_default()
    }

    pub(super) fn snippet(&self, text: &str, terms: &[String]) -> String {
        snippet_with(self.index.tokenizers().get(ANALYZER).as_mut(), text, terms)
    }

    pub(super) fn fact_excerpt(&self, fact: &Fact, query: &str) -> String {
        let terms: Vec<_> = self
            .terms_of(query)
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        excerpt::of_fact(fact, |text| self.matches(text, &terms))
    }
}

fn matches_with(
    analyzer: &mut tantivy::tokenizer::TextAnalyzer,
    text: &str,
    terms: &[String],
) -> Vec<Match> {
    let mut stream = analyzer.token_stream(text);
    let mut found = Vec::new();
    while stream.advance() {
        let token = stream.token();
        for (term, wanted) in terms.iter().enumerate() {
            if token.text == *wanted {
                found.push(Match {
                    term,
                    start: token.offset_from,
                    end: token.offset_to,
                });
            }
        }
    }
    found
}

pub(super) fn snippet_with(
    analyzer: Option<&mut tantivy::tokenizer::TextAnalyzer>,
    text: &str,
    terms: &[String],
) -> String {
    excerpt::snippet(
        text,
        &analyzer
            .map(|analyzer| matches_with(analyzer, text, terms))
            .unwrap_or_default(),
    )
}
