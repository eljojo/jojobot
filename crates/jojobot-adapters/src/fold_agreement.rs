//! **The fold, held to the store by command sequences nobody wrote by hand.**
//!
//! A seeded generator draws a sequence of writes over the memory verbs (an
//! entity added, a claim captured or edited or taken back, two things merged, a
//! thing renamed or archived or restored, a type declared) from the roster, and
//! applies each through the running fold. When the sequence ends, what the fold
//! serves is compared, read by read and in the order it is served, with what a
//! fold filled anew from the store serves and with what the store serves itself.
//! A disagreement names the seed, the step that led to it, the read and the
//! thing, and the seed alone replays it.
//!
//! The walks in [`crate::fold_battery`] check the writes somebody thought of.
//! This draws the ones nobody did: a merge between two claims, a rename after a
//! declaration, a record edited twice.

use std::collections::BTreeSet;
use std::sync::Arc;

use jiff::civil::date;
use jojobot_domain::memory::testing::contract::{state, state_patch};
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use jojobot_domain::memory::{
    EntityId, FactAddress, FactPatch, Guarded, Memory, NewEntity, NewFact, Override, Provenance,
};

/// A small seeded generator (splitmix64): the same seed draws the same run on any
/// host, with no dependency.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `n`.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        (!items.is_empty()).then(|| &items[self.below(items.len())])
    }
}

/// The handles a run draws on, all from the fictional roster. Each kind has more
/// than a run uses, so a rename has a free handle to move to.
const POOL: &[&str] = &[
    "person:homer",
    "person:bart",
    "person:lisa",
    "person:marge",
    "person:ralph",
    "person:milhouse",
    "thing:delta",
    "thing:epsilon",
    "thing:gamma",
    "thing:sigma",
    "thing:tau",
    "thing:omega",
    "work:delta",
    "work:epsilon",
    "work:gamma",
    "work:phi",
    "work:sigma",
    "work:tau",
];

/// The keys a claim carries. The first three can name other things.
const KEYS: &[&str] = &["blocks", "owner", "tags", "status", "rank"];

/// What a run did, for a report.
#[derive(Debug, Default)]
pub struct Trace {
    /// One line for each step, in order: what was tried and how it came out.
    pub steps: Vec<String>,
    /// How many steps the store refused or blocked.
    pub refused: usize,
}

/// The kind of a handle.
fn kind_of(handle: &str) -> &str {
    handle.split(':').next().unwrap_or("")
}

/// What the generator knows of the store it is writing to.
#[derive(Default)]
struct Model {
    /// Things that exist and have not been merged away.
    live: Vec<String>,
    /// Handles taken, merged away or renamed from: never drawn again.
    spent: BTreeSet<String>,
    /// Claims written, by address.
    facts: Vec<FactAddress>,
    /// Things archived.
    archived: Vec<String>,
    declared: bool,
}

fn id(handle: &str) -> EntityId {
    EntityId(handle.to_string())
}

/// **Draw and apply `steps` writes through `w` from `seed`.** A write the store
/// refuses is counted and skipped; the run goes on.
pub async fn run_sequence(seed: u64, steps: usize, w: &dyn Memory) -> Trace {
    let mut rng = Rng::new(seed);
    let mut model = Model::default();
    let mut trace = Trace::default();
    let caller = id("bot:sigma");
    for n in 0..steps {
        // Until there are things, only things are added.
        let draw = if model.live.len() < 3 {
            0
        } else {
            rng.below(100)
        };
        let line = match draw {
            0..=14 => add(&mut rng, &mut model, w).await,
            15..=54 => capture(&mut rng, &mut model, w, seed, n).await,
            55..=69 => edit(&mut rng, &model, w, &caller).await,
            70..=76 => retract(&mut rng, &mut model, w, &caller).await,
            77..=84 => merge(&mut rng, &mut model, w, &caller).await,
            85..=90 => rename(&mut rng, &mut model, w).await,
            91..=94 => archive(&mut rng, &mut model, w).await,
            _ => declare(&mut model, w).await,
        };
        if line.contains("refused") {
            trace.refused += 1;
        }
        trace.steps.push(format!("{n}: {line}"));
    }
    trace
}

async fn add(rng: &mut Rng, model: &mut Model, w: &dyn Memory) -> String {
    let free: Vec<&&str> = POOL
        .iter()
        .filter(|h| !model.live.iter().any(|l| l == **h) && !model.spent.contains(**h))
        .collect();
    let Some(handle) = rng.pick(&free).map(|h| (**h).to_string()) else {
        return "add: no free handle".into();
    };
    let new = NewEntity::new(id(&handle), &handle, "fold-agreement");
    let landed = match w.add_entity(new.clone()).await {
        Ok(Guarded::Blocked {
            attempted,
            candidates,
        }) => w
            .add_entity(NewEntity {
                override_token: Some(jojobot_domain::memory::guard::override_token(
                    &attempted,
                    &candidates,
                )),
                override_reason: Some("a fixture sharing a name".into()),
                ..new
            })
            .await
            .map(|g| g.written().is_some()),
        Ok(other) => Ok(other.written().is_some()),
        Err(e) => Err(e),
    };
    match landed {
        Ok(true) => {
            model.live.push(handle.clone());
            format!("add {handle}")
        }
        _ => format!("add {handle}: refused"),
    }
}

/// A value for a key: a handle, a list of two, or a scalar.
fn value_for(rng: &mut Rng, model: &Model, key: &str) -> String {
    match key {
        "blocks" | "owner" | "tags" => {
            let one = rng.pick(&model.live).cloned().unwrap_or_default();
            if key == "tags" && rng.below(2) == 0 {
                let two = rng.pick(&model.live).cloned().unwrap_or_default();
                format!("kitchen, {one}, {two}")
            } else if rng.below(3) == 0 {
                let two = rng.pick(&model.live).cloned().unwrap_or_default();
                format!("{one}, {two}")
            } else {
                one
            }
        }
        "rank" => rng.below(5).to_string(),
        _ => ["now", "next", "done"][rng.below(3)].to_string(),
    }
}

fn some_fields(rng: &mut Rng, model: &Model) -> Vec<(String, String)> {
    let mut chosen = Vec::new();
    for _ in 0..rng.below(4) {
        let key = KEYS[rng.below(KEYS.len())];
        if !chosen.iter().any(|(k, _): &(String, String)| k == key) {
            chosen.push((key.to_string(), value_for(rng, model, key)));
        }
    }
    chosen
}

async fn capture(rng: &mut Rng, model: &mut Model, w: &dyn Memory, seed: u64, n: usize) -> String {
    let Some(subject) = rng.pick(&model.live).cloned() else {
        return "capture: nothing to write on".into();
    };
    let fields = some_fields(rng, model);
    let mut fact = state(NewFact {
        fields: fields.iter().cloned().collect(),
        ..NewFact::about(
            id(&subject),
            format!("seed {seed} step {n} on {subject}"),
            date(2026, 9, 1),
        )
    });
    fact.session = Some("fold-agreement".into());
    let said = format!("capture on {subject} {fields:?}");
    match w.capture(fact).await {
        Ok(Guarded::Written(fact)) => {
            model.facts.push(fact.address());
            said
        }
        _ => format!("{said}: refused"),
    }
}

async fn edit(rng: &mut Rng, model: &Model, w: &dyn Memory, caller: &EntityId) -> String {
    let Some(address) = rng.pick(&model.facts).cloned() else {
        return "edit: no claim yet".into();
    };
    let fields = some_fields(rng, model);
    let clear: Vec<String> = if rng.below(3) == 0 {
        vec![KEYS[rng.below(KEYS.len())].to_string()]
    } else {
        Vec::new()
    };
    let said = format!("edit {address:?} set {fields:?} clear {clear:?}");
    let patch = state_patch(FactPatch {
        provenance: Some(Provenance::Inference),
        fields: fields.into_iter().collect(),
        clear_fields: clear,
        ..Default::default()
    });
    match w.update_fact(&address, patch, caller).await {
        Ok(Guarded::Written(_)) => said,
        _ => format!("{said}: refused"),
    }
}

async fn retract(rng: &mut Rng, model: &mut Model, w: &dyn Memory, caller: &EntityId) -> String {
    if model.facts.is_empty() {
        return "retract: no claim yet".into();
    }
    let at = rng.below(model.facts.len());
    let address = model.facts.remove(at);
    let said = format!("retract {address:?}");
    match w
        .retract(&address, Some("a mistake"), date(2026, 9, 2), caller, None)
        .await
    {
        Ok(_) => said,
        Err(_) => format!("{said}: refused"),
    }
}

async fn merge(rng: &mut Rng, model: &mut Model, w: &dyn Memory, caller: &EntityId) -> String {
    let Some(folded) = rng.pick(&model.live).cloned() else {
        return "merge: nothing to merge".into();
    };
    let mates: Vec<String> = model
        .live
        .iter()
        .filter(|h| **h != folded && kind_of(h) == kind_of(&folded))
        .cloned()
        .collect();
    let Some(survivor) = rng.pick(&mates).cloned() else {
        return format!("merge {folded}: no mate");
    };
    let said = format!("merge {folded} into {survivor}");
    match w
        .merge(
            &id(&folded),
            &id(&survivor),
            Some("the same one"),
            date(2026, 9, 3),
            caller,
            Some("fold-agreement"),
        )
        .await
    {
        Ok(_) => {
            model.live.retain(|h| *h != folded);
            model.spent.insert(folded);
            said
        }
        Err(_) => format!("{said}: refused"),
    }
}

async fn rename(rng: &mut Rng, model: &mut Model, w: &dyn Memory) -> String {
    let Some(from) = rng.pick(&model.live).cloned() else {
        return "rename: nothing to rename".into();
    };
    let free: Vec<&&str> = POOL
        .iter()
        .filter(|h| {
            kind_of(h) == kind_of(&from)
                && !model.live.iter().any(|l| l == **h)
                && !model.spent.contains(**h)
        })
        .collect();
    let Some(to) = rng.pick(&free).map(|h| (**h).to_string()) else {
        return format!("rename {from}: no free handle");
    };
    let said = format!("rename {from} to {to}");
    match w
        .rename_entity(
            &id(&from),
            &id(&to),
            None,
            date(2026, 9, 4),
            Override::none(),
            Some("fold-agreement"),
        )
        .await
    {
        Ok(Guarded::Written(_)) => {
            model.live.retain(|h| *h != from);
            model.spent.insert(from);
            model.live.push(to);
            said
        }
        _ => format!("{said}: refused"),
    }
}

async fn archive(rng: &mut Rng, model: &mut Model, w: &dyn Memory) -> String {
    if let Some(back) = model.archived.pop() {
        let said = format!("restore {back}");
        return match w.restore_entity(&id(&back), Some("fold-agreement")).await {
            Ok(_) => said,
            Err(_) => format!("{said}: refused"),
        };
    }
    let Some(handle) = rng.pick(&model.live).cloned() else {
        return "archive: nothing to archive".into();
    };
    let said = format!("archive {handle}");
    match w
        .archive_entity(&id(&handle), "set aside", Some("fold-agreement"))
        .await
    {
        Ok(_) => {
            model.archived.push(handle);
            said
        }
        Err(_) => format!("{said}: refused"),
    }
}

async fn declare(model: &mut Model, w: &dyn Memory) -> String {
    if std::mem::replace(&mut model.declared, true) {
        return "declare: already".into();
    }
    match w
        .declare_type(
            DeclaredType::new(
                "agree-tagged",
                vec![Field::listing("tags", ValueType::Reference)],
            ),
            None,
        )
        .await
    {
        Ok(_) => "declare tags a list of references".into(),
        Err(_) => "declare: refused".into(),
    }
}

/// **The first read two memories answer differently**, with the thing and the
/// reads' answers; `None` when they agree on every read, in order.
pub async fn disagreement(left: &dyn Memory, right: &dyn Memory) -> Option<String> {
    macro_rules! same {
        ($what:expr, $thing:expr, $a:expr, $b:expr) => {{
            let (a, b) = (format!("{:?}", $a), format!("{:?}", $b));
            if a != b {
                let cut = |s: &str| s.chars().take(600).collect::<String>();
                return Some(format!(
                    "{} of {}:\n  left:  {}\n  right: {}",
                    $what,
                    $thing,
                    cut(&a),
                    cut(&b)
                ));
            }
        }};
    }
    same!(
        "list_entities",
        "everything",
        left.list_entities(None).await,
        right.list_entities(None).await
    );
    same!(
        "former_handles",
        "everything",
        left.former_handles().await,
        right.former_handles().await
    );
    same!(
        "merge_events",
        "everything",
        left.merge_events(None).await,
        right.merge_events(None).await
    );
    same!(
        "declared_types",
        "everything",
        left.declared_types().await,
        right.declared_types().await
    );
    let all = right.list_entities(None).await.unwrap_or_default();
    for entity in &all {
        let thing = entity.id.to_string();
        same!(
            "recall",
            thing,
            left.recall(&entity.id).await,
            right.recall(&entity.id).await
        );
        same!(
            "fields",
            thing,
            left.fields(&entity.id).await,
            right.fields(&entity.id).await
        );
        same!(
            "referring_to",
            thing,
            left.referring_to(&entity.id).await,
            right.referring_to(&entity.id).await
        );
        same!(
            "children",
            thing,
            left.children(&entity.id).await,
            right.children(&entity.id).await
        );
    }
    None
}

/// **One seed over one store**: run the sequence through a fold, then compare
/// the fold with a fold filled anew and with the store. `Err` carries the seed,
/// the steps and the first read that differs, so the seed replays it.
pub async fn check_seed(seed: u64, steps: usize, store: Arc<dyn Memory>) -> Result<(), String> {
    let folded = crate::fold::Folded::new(store.clone());
    folded.rebuild().await.expect("the fold loads");
    let trace = run_sequence(seed, steps, &folded).await;
    let fresh = crate::fold::Folded::new(store.clone());
    fresh.rebuild().await.expect("the fold loads again");
    for (against, other) in [
        ("a fold filled anew", &fresh as &dyn Memory),
        ("the store", &*store),
    ] {
        if let Some(differs) = disagreement(&folded, other).await {
            return Err(format!(
                "seed {seed}: the running fold differs from {against}\n{differs}\nsteps:\n  {}",
                trace.steps.join("\n  ")
            ));
        }
    }
    Ok(())
}
