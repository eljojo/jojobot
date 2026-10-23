use super::*;
use jojobot_domain::memory::search::excerpt::{self, Match};

impl FullTextIndex {
    fn matches(&self, text: &str, terms: &[String]) -> Vec<Match> {
        let Some(mut analyzer) = self.index.tokenizers().get(ANALYZER) else {
            return Vec::new();
        };
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

    pub(super) fn snippet(&self, text: &str, terms: &[String]) -> String {
        excerpt::snippet(text, &self.matches(text, terms))
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
