//! **A projection family computed two ways on every read, served the old way.**
//!
//! A projection family is a kind of read the fold answers from its image. When a
//! family is moved (answered by the store, or by a new rule), the move is proven
//! on real reads before it is served: with the family's shadow on, each read of
//! it is answered the old way, from the image, and also the new way, from the
//! store, and the two are compared. Each disagreement is counted and logged with
//! the thing asked about and the field that differs. The answer a caller gets is
//! the old one, so a shadow that finds a disagreement changes nothing a caller
//! sees.
//!
//! **Off by default.** A family is shadowed only when it is named in the
//! configuration (`JOJOBOT_SHADOW`, a comma-separated list of family names), and
//! a name no family answers to refuses the start rather than shadowing nothing.

use std::sync::atomic::{AtomicU64, Ordering};

use jojobot_domain::memory::{EntityId, Fact};

/// TEMPORARY(9.3.3) the referring_to shadow, JOJOBOT_SHADOW. The projection families that can be shadowed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// `referring_to`: who points at a thing.
    ReferringTo,
}

impl Family {
    const ALL: [Family; 1] = [Family::ReferringTo];

    /// The name the configuration gives it.
    pub fn name(self) -> &'static str {
        match self {
            Family::ReferringTo => "referring_to",
        }
    }
}

/// What a shadowed family has counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counts {
    pub family: Family,
    /// Reads of the family that were compared.
    pub reads: u64,
    /// Of those, reads whose two answers differed.
    pub disagreements: u64,
}

/// Which families are shadowed, and what each has counted. Empty by default.
#[derive(Default)]
pub struct Shadow {
    watched: Vec<(Family, AtomicU64, AtomicU64)>,
}

impl Shadow {
    /// The families a configuration value names. An empty value names none.
    pub fn named(spec: &str) -> Result<Shadow, String> {
        let mut watched = Vec::new();
        for name in spec.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            let family = Family::ALL
                .iter()
                .copied()
                .find(|family| family.name() == name)
                .ok_or_else(|| {
                    let known: Vec<&str> = Family::ALL.iter().map(|f| f.name()).collect();
                    format!("{name:?} is no projection family; the families are {known:?}")
                })?;
            if !watched.iter().any(|(f, _, _)| *f == family) {
                watched.push((family, AtomicU64::new(0), AtomicU64::new(0)));
            }
        }
        Ok(Shadow { watched })
    }

    /// Whether this family is shadowed.
    pub fn watches(&self, family: Family) -> bool {
        self.watched.iter().any(|(f, _, _)| *f == family)
    }

    /// What each shadowed family has counted.
    pub fn counts(&self) -> Vec<Counts> {
        self.watched
            .iter()
            .map(|(family, reads, disagreements)| Counts {
                family: *family,
                reads: reads.load(Ordering::Relaxed),
                disagreements: disagreements.load(Ordering::Relaxed),
            })
            .collect()
    }

    /// **Compare the two answers to one `referring_to` read**, count it, and log
    /// the difference when there is one.
    pub fn compared_referring_to(&self, thing: &EntityId, served: &[Fact], other: &[Fact]) {
        let Some((family, reads, disagreements)) = self
            .watched
            .iter()
            .find(|(f, _, _)| *f == Family::ReferringTo)
        else {
            return;
        };
        reads.fetch_add(1, Ordering::Relaxed);
        if let Some(field) = first_difference(served, other) {
            disagreements.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(
                family = family.name(),
                thing = %thing,
                field = %field,
                "shadow: the image and the store answer a read differently (the image is served)"
            );
        }
    }
}

/// The first field two lists of records differ in, or `None` when they are the
/// same list: the records only one side holds, then the order, then the first
/// record whose fields differ.
fn first_difference(served: &[Fact], other: &[Fact]) -> Option<String> {
    let key = |fact: &Fact| (fact.home.to_string(), fact.id.to_string());
    let (a, b): (Vec<_>, Vec<_>) = (
        served.iter().map(key).collect(),
        other.iter().map(key).collect(),
    );
    let only_served: Vec<_> = a.iter().filter(|k| !b.contains(k)).collect();
    let only_other: Vec<_> = b.iter().filter(|k| !a.contains(k)).collect();
    if !only_served.is_empty() || !only_other.is_empty() {
        return Some(format!(
            "holders: only the image has {only_served:?}, only the store has {only_other:?}"
        ));
    }
    if a != b {
        return Some("order".to_string());
    }
    served
        .iter()
        .zip(other)
        .find(|(x, y)| x != y)
        .map(|(x, _)| format!("record {}#{}", x.home, x.id))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use jiff::civil::date;
    use jojobot_domain::memory::testing::{InMemoryMemory, contract::state};
    use jojobot_domain::memory::{Memory, NewEntity, NewFact};

    use super::*;
    use crate::counted::Counted;
    use crate::fold::Folded;

    fn id(handle: &str) -> EntityId {
        EntityId(handle.to_string())
    }

    async fn thing(w: &dyn Memory, handle: &str) {
        w.add_entity(NewEntity::new(id(handle), handle, "shadow"))
            .await
            .expect("add")
            .written()
            .expect("written");
    }

    async fn claim(w: &dyn Memory, about: &str, blocks: &str, content: &str) {
        let mut fact = state(NewFact {
            fields: [("blocks".to_string(), blocks.to_string())].into(),
            ..NewFact::about(id(about), content, date(2026, 9, 1))
        });
        fact.session = Some("shadow".into());
        w.capture(fact)
            .await
            .expect("capture")
            .written()
            .expect("written");
    }

    /// **A forced disagreement is counted and logged with the thing and the
    /// field, and the image's answer is the one served.** The store takes a claim
    /// the fold never saw, so the two answer a read differently. **Paired**: the
    /// read before it agrees and is counted as a read alone.
    #[tokio::test]
    async fn a_forced_disagreement_is_counted_and_logged_and_the_image_is_served() {
        crate::testing::install_store_log_sink();
        let store = Arc::new(InMemoryMemory::booted());
        let folded = Folded::new(store.clone())
            .shadowing("referring_to")
            .expect("a family");
        folded.rebuild().await.expect("the fold loads");
        thing(&folded, "work:phi").await;
        thing(&folded, "thing:omega").await;
        claim(&folded, "thing:omega", "work:phi", "omega blocks phi").await;

        let agreed = folded.referring_to(&id("work:phi")).await.expect("read");
        assert_eq!(agreed.len(), 1);
        let counts = folded.shadow_counts();
        assert_eq!(
            (counts[0].reads, counts[0].disagreements),
            (1, 0),
            "an agreeing read is counted as a read and as no disagreement: {counts:?}"
        );

        // A claim written under the fold's feet: the store holds it, the image does not.
        claim(&*store, "thing:omega", "work:phi", "omega blocks phi again").await;
        let served = folded.referring_to(&id("work:phi")).await.expect("read");
        assert_eq!(served.len(), 1, "the image's answer is the one served");
        let counts = folded.shadow_counts();
        assert_eq!(
            (counts[0].reads, counts[0].disagreements),
            (2, 1),
            "{counts:?}"
        );
        let log = crate::testing::store_log();
        assert!(
            log.contains("shadow") && log.contains("work:phi") && log.contains("holders"),
            "the disagreement is logged with the thing and the field: {log}"
        );
    }

    /// **Off by default, and a family no one answers to is refused.**
    #[tokio::test]
    async fn shadow_is_off_unless_a_family_is_named_and_a_wrong_name_is_refused() {
        let store = Arc::new(InMemoryMemory::booted());
        let counted = Arc::new(Counted::new(store));
        let folded = Folded::new(counted.clone());
        folded.rebuild().await.expect("the fold loads");
        thing(&folded, "work:phi").await;
        counted.reset();
        folded.referring_to(&id("work:phi")).await.expect("read");
        assert_eq!(
            counted.seen().get("referring_to").copied().unwrap_or(0),
            0,
            "with no family named the store is not asked"
        );
        assert!(folded.shadow_counts().is_empty());
        assert!(Shadow::named("").expect("none").counts().is_empty());
        assert!(Shadow::named("no_such_family").is_err());
    }
}
