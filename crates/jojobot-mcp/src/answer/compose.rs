//! **The answer composer** — one place that decides which of an answer's
//! optional parts ship, and names the ones that did not.
//!
//! An answer is a core and a list of named layers. The core is always served
//! whole. Layers fill to the bot's budget by relevance, at most one of them a
//! suggestion, and each one left out is named once with the call that returns
//! it. The account of what was left out is part of the answer and is counted
//! inside the budget; when even the core and the bare names do not fit, the
//! answer is served over budget and says so. A tighter budget only ever drops
//! layers: the shape never changes (rules 692, 694, 695).
//!
//! Pure: no I/O, no setting read, no teaching ledger read. The caller passes
//! the budget level and the domains the session was already taught.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

/// Key under which the omissions ride, present only when a layer was left out.
pub(crate) const LEFT_OUT_KEY: &str = "left_out";

/// What a layer is. Only a note carries a teaching domain; only one
/// suggestion ships (rule 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Note,
    Suggestion,
    Detail,
}

/// How much a caller needs a layer. The first tier fills first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Tier {
    High,
    Medium,
    Low,
}

/// A part of an answer that can be left out.
#[derive(Clone, Debug)]
pub(crate) struct Layer {
    /// Stable name: the key the layer is served under, and how an omission names it.
    pub(crate) name: &'static str,
    pub(crate) kind: Kind,
    pub(crate) tier: Tier,
    /// Served characters, by the door's own measure ([`super::served_len`]) of
    /// the layer as it rides under its key.
    pub(crate) cost: usize,
    /// A once-per-session note names its teaching domain; a session already
    /// taught that domain does not see the layer, nor an omission for it.
    pub(crate) domain: Option<&'static str>,
    /// The call that returns the layer when it is left out.
    pub(crate) returned_by: &'static str,
    pub(crate) body: Value,
}

impl Layer {
    pub(crate) fn new(
        name: &'static str,
        kind: Kind,
        tier: Tier,
        returned_by: &'static str,
        body: Value,
    ) -> Self {
        let cost = super::served_len(&json!({ name: body }));
        Layer {
            name,
            kind,
            tier,
            cost,
            domain: None,
            returned_by,
            body,
        }
    }

    pub(crate) fn taught_once(mut self, domain: &'static str) -> Self {
        self.domain = Some(domain);
        self
    }
}

/// A verb's whole answer before the budget decides what ships.
#[derive(Clone, Debug)]
pub(crate) struct Answer {
    core: Map<String, Value>,
    layers: Vec<Layer>,
}

impl Answer {
    /// # Panics
    /// On a defect of the verb, never of a caller: a core that is not an
    /// object, or a layer whose name another layer or a core key already holds.
    pub(crate) fn new(core: Value, layers: Vec<Layer>) -> Self {
        let Value::Object(core) = core else {
            panic!("an answer's core is an object");
        };
        let mut names = BTreeSet::new();
        for layer in &layers {
            assert!(
                names.insert(layer.name) && !core.contains_key(layer.name),
                "layer name {} is held twice",
                layer.name
            );
        }
        assert!(
            [LEFT_OUT_KEY, OVER_BUDGET_KEY]
                .iter()
                .all(|key| !names.contains(key) && !core.contains_key(*key)),
            "{LEFT_OUT_KEY} and {OVER_BUDGET_KEY} are the composer's keys"
        );
        Answer { core, layers }
    }
}

/// How strict the bot's setting makes the answer. Two levels; a call never
/// picks one (rule 695).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Budget {
    /// `setting_token_saving` off, the default.
    Lean,
    /// `setting_token_saving` on.
    Strict,
}

impl Budget {
    pub(crate) fn of_setting(token_saving: bool) -> Self {
        if token_saving {
            Budget::Strict
        } else {
            Budget::Lean
        }
    }

    /// Served characters the whole answer, core included, may take.
    pub(crate) fn limit(self) -> usize {
        match self {
            Budget::Lean => LEAN_LIMIT,
            Budget::Strict => STRICT_LIMIT,
        }
    }
}

/// Measured from the receipts today's capture serves; see the card's report.
pub(crate) const LEAN_LIMIT: usize = 2400;
pub(crate) const STRICT_LIMIT: usize = 160;

/// An omission: the layer's name and the call that returns it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LeftOut {
    pub(crate) layer: &'static str,
    pub(crate) call: &'static str,
}

/// Key beside the omissions that says the answer is over its budget, present
/// only then. Its value is the size of the floor.
pub(crate) const OVER_BUDGET_KEY: &str = "over_budget";

/// How the account of what was left out is written, richest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Form {
    /// Each omitted layer by name, with the call that returns it.
    Full,
    /// The names gathered under each distinct call, each call written once.
    Grouped,
    /// The names alone, and each distinct call once, in the order its layer was
    /// left out.
    Floor,
}

impl Form {
    pub(crate) const RICHEST_FIRST: [Form; 3] = [Form::Full, Form::Grouped, Form::Floor];
}

/// What the omissions are written as under [`LEFT_OUT_KEY`], in one form.
pub(crate) fn account(omitted: &[LeftOut], form: Form) -> Value {
    match form {
        Form::Full => Value::Array(
            omitted
                .iter()
                .map(|omitted| json!({ "layer": omitted.layer, "call": omitted.call }))
                .collect(),
        ),
        Form::Grouped => {
            let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
            for omitted in omitted {
                match groups.iter_mut().find(|(call, _)| *call == omitted.call) {
                    Some((_, layers)) => layers.push(omitted.layer),
                    None => groups.push((omitted.call, vec![omitted.layer])),
                }
            }
            Value::Array(
                groups
                    .into_iter()
                    .map(|(call, layers)| json!({ "call": call, "layers": layers }))
                    .collect(),
            )
        }
        Form::Floor => {
            let mut calls: Vec<&str> = Vec::new();
            for omitted in omitted {
                if !calls.contains(&omitted.call) {
                    calls.push(omitted.call);
                }
            }
            json!({
                "layers": omitted.iter().map(|omitted| omitted.layer).collect::<Vec<_>>(),
                "calls": calls.join(", "),
            })
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Composed {
    pub(crate) served: Value,
    pub(crate) left_out: Vec<LeftOut>,
    /// The form the account of `left_out` is written in; `Full` when nothing
    /// was left out.
    pub(crate) form: Form,
    /// Set when even the floor does not fit the limit: the floor's size.
    pub(crate) over_budget: Option<usize>,
}

pub fn compose(answer: &Answer, budget: Budget, taught: &BTreeSet<&str>) -> Composed {
    compose_within(answer, budget.limit(), taught)
}

/// [`compose`] against a number of characters, so the proof can walk every limit.
pub(crate) fn compose_within(answer: &Answer, limit: usize, taught: &BTreeSet<&str>) -> Composed {
    // A taught once-per-session note is dropped: no layer, no omission
    // (table: taught_domain).
    let mut ranked: Vec<&Layer> = answer
        .layers
        .iter()
        .filter(|layer| layer.domain.is_none_or(|domain| !taught.contains(domain)))
        .collect();
    // Tier first, then the cheaper layer (more relevance per character);
    // the sort is stable, so ties keep declared order (table: rank_order).
    ranked.sort_by_key(|layer| (layer.tier, layer.cost));

    // Only the best-ranked suggestion may ship (table: one_suggestion); a
    // later one is left out whatever the room.
    let mut first_suggestion = true;
    let eligible: Vec<bool> = ranked
        .iter()
        .map(|layer| match layer.kind {
            Kind::Suggestion => std::mem::replace(&mut first_suggestion, false),
            _ => true,
        })
        .collect();
    let fillable = eligible.iter().filter(|eligible| **eligible).count();

    // The fill is a prefix of the eligible ranking, and the account of the
    // rest is paid for inside the limit (table: prefix_fill). The largest
    // prefix that fits wins, so a smaller limit serves a subset of a larger
    // one's layers.
    let attempt = |k: usize, form: Form| {
        let mut served = answer.core.clone();
        let mut left_out = Vec::new();
        let mut taken = 0;
        let mut layers_cost = 0;
        for (layer, eligible) in ranked.iter().zip(&eligible) {
            if *eligible && taken < k {
                taken += 1;
                layers_cost += layer.cost;
                served.insert(layer.name.to_string(), layer.body.clone());
            } else {
                left_out.push(LeftOut {
                    layer: layer.name,
                    call: layer.returned_by,
                });
            }
        }
        if !left_out.is_empty() {
            served.insert(LEFT_OUT_KEY.to_string(), account(&left_out, form));
        }
        // The core and the account as they stand, plus what each served layer
        // is charged: the charge the fill has always made, which is one
        // character over what a layer adds to an object.
        let mut bare = answer.core.clone();
        if let Some(account) = served.get(LEFT_OUT_KEY) {
            bare.insert(LEFT_OUT_KEY.to_string(), account.clone());
        }
        let len = super::served_len(&Value::Object(bare)) + layers_cost;
        (Value::Object(served), left_out, len)
    };
    for k in (0..=fillable).rev() {
        for form in Form::RICHEST_FIRST {
            let (served, left_out, len) = attempt(k, form);
            if len <= limit {
                return Composed {
                    served,
                    left_out,
                    form,
                    over_budget: None,
                };
            }
            // With nothing left out the forms are one and the same answer.
            if left_out.is_empty() {
                break;
            }
        }
    }
    // Not even the floor fits: serve it anyway and say how big it is
    // (table: core_over_budget).
    let (mut served, left_out, floor) = attempt(0, Form::Floor);
    if let Value::Object(served) = &mut served {
        served.insert(OVER_BUDGET_KEY.to_string(), json!(floor));
    }
    Composed {
        served,
        left_out,
        form: Form::Floor,
        over_budget: Some(floor),
    }
}
