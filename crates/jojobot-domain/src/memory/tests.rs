use super::testing::{InMemoryMemory, contract};
use super::*;

fn thing(handle: &str, badge: Option<&str>) -> Entity {
    Entity {
        id: EntityId(handle.into()),
        kind: EntityId(handle.into()).kind().expect("a test handle"),
        name: handle.into(),
        aliases: Vec::new(),
        source: "the fixture roster".into(),
        crm: None,
        parent: None,
        boot: Boot::OnDemand,
        merged_into: None,
        badge: badge.map(str::to_string),
        archived: None,
    }
}

fn at(s: &str) -> jiff::Timestamp {
    s.parse().expect("a fixed instant")
}

/// **Both of a role's own fields are caught, by shape, and an ordinary
/// field never is.** The positive and the negative in one read: a write
/// naming neither role field has nothing here to refuse. The old shape is
/// caught on any subject, because it is a key written on a bot.
#[test]
fn refuses_role_fields_catches_both_of_a_roles_fields_and_nothing_else() {
    let bot = EntityId("bot:gamma".into());
    let holder = crate::session::role_holder_key("dev-dispatch");
    let claimed_at = crate::session::role_claimed_at_key("dev-dispatch");
    let ordinary = "thought_capacity".to_string();

    let refused = refuses_role_fields(&bot, [&holder]);
    assert!(
        matches!(&refused, Some(MemoryError::RoleFieldGuarded { role, key }) if role == "dev-dispatch" && key == &holder),
        "{refused:?}",
    );
    let refused = refuses_role_fields(&bot, [&claimed_at]);
    assert!(
        matches!(&refused, Some(MemoryError::RoleFieldGuarded { role, key }) if role == "dev-dispatch" && key == &claimed_at),
        "{refused:?}",
    );
    assert!(refuses_role_fields(&bot, [&ordinary]).is_none());
    assert!(refuses_role_fields(&bot, std::iter::empty::<&String>()).is_none());
}

/// **A role object's holder and claim moment are guarded on the role object,
/// and nowhere else.** The agent key and a watcher's mark are plain keys the
/// line and the watchers write, so they are not caught; a person, or a bot, that
/// carries a field called `holder` is not a role and is not caught either. The
/// negatives are what keep a guard that refused every write on a role from
/// passing the positives.
#[test]
fn a_role_objects_holder_and_moment_are_guarded_on_that_object_alone() {
    let role = EntityId("role:dev-dispatch".into());
    let person = EntityId("person:milhouse".into());
    for key in [crate::session::ROLE_HOLDER, crate::session::ROLE_CLAIMED_AT] {
        let key = key.to_string();
        let refused = refuses_role_fields(&role, [&key]);
        assert!(
            matches!(&refused, Some(MemoryError::RoleFieldGuarded { role, key: k }) if role == "dev-dispatch" && k == &key),
            "{key}: {refused:?}",
        );
        assert!(
            refuses_role_fields(&person, [&key]).is_none(),
            "{key} on a person is an ordinary key",
        );
    }
    for plain in ["agent", "probed_at", "first_seen_empty"] {
        let key = plain.to_string();
        assert!(
            refuses_role_fields(&role, [&key]).is_none(),
            "{plain} is written by the line or a watcher and lands",
        );
    }
}

/// **A fold is served with its manager as the handle that manager answers to,
/// and a fold with no manager is served untouched.** Both stores keep the
/// permanent id under `reports_to`, and the chart is judged in handles, so every
/// chart question goes through this one step. The lookup is the store's own;
/// the step is that the stored value is replaced by what the lookup answers, and
/// that nothing is looked up when there is nothing to look up.
#[test]
fn a_fold_is_served_with_its_manager_as_the_handle_it_answers_to() {
    let fold: BTreeMap<String, String> = [
        (REPORTS_TO.to_string(), "kqde3z".to_string()),
        ("one_liner".to_string(), "keeps the books".to_string()),
    ]
    .into_iter()
    .collect();
    let served = with_manager_served(&fold, |stored| {
        assert_eq!(
            stored.as_str(),
            "kqde3z",
            "the lookup is asked for the stored id"
        );
        EntityId("bot:delta".into())
    });
    assert_eq!(
        served.get(REPORTS_TO).map(String::as_str),
        Some("bot:delta")
    );
    assert_eq!(
        served.get("one_liner"),
        fold.get("one_liner"),
        "every other key is served as it was",
    );

    // **A store that keeps the id behind the mention mark is asked the same
    // question**, so the lookup is never handed a mark it would not find.
    let marked: BTreeMap<String, String> = [(
        REPORTS_TO.to_string(),
        format!("{}kqde3z", crate::memory::mention::MARK),
    )]
    .into_iter()
    .collect();
    let served = with_manager_served(&marked, |stored| {
        assert_eq!(
            stored.as_str(),
            "kqde3z",
            "the mark is taken off before the lookup"
        );
        EntityId("bot:delta".into())
    });
    assert_eq!(
        served.get(REPORTS_TO).map(String::as_str),
        Some("bot:delta")
    );

    let unmanaged: BTreeMap<String, String> =
        [("one_liner".to_string(), "keeps the books".to_string())]
            .into_iter()
            .collect();
    let served = with_manager_served(&unmanaged, |_| panic!("looked up a manager nobody holds"));
    assert_eq!(served, unmanaged);
}

/// **A folded bot is followed to the one it became, and a forwarding that loops
/// ends.** A row that forwards nowhere is its own survivor; one that forwards on
/// is followed to the end; and two rows forwarding to each other end the walk at
/// the bound instead of hanging the write that asked.
#[test]
fn a_folded_bot_is_followed_to_the_one_it_became_and_a_loop_ends() {
    let id = |s: &str| EntityId(format!("bot:{s}"));
    let forwards = |from: &str, to: &str| {
        let (from, to) = (id(from), id(to));
        move |at: &EntityId| (at == &from).then(|| to.clone())
    };
    assert_eq!(
        survivor_of(id("ridge"), |_| None),
        id("ridge"),
        "a row that forwards nowhere is its own survivor",
    );
    assert_eq!(
        survivor_of(id("spur"), forwards("spur", "ridge")),
        id("ridge")
    );

    let chain = |at: &EntityId| match at.as_str() {
        "bot:delta" => Some(id("gamma")),
        "bot:gamma" => Some(id("omega")),
        _ => None,
    };
    assert_eq!(
        survivor_of(id("delta"), chain),
        id("omega"),
        "followed past one hop"
    );

    let loops = |at: &EntityId| {
        Some(if at.as_str() == "bot:delta" {
            id("gamma")
        } else {
            id("delta")
        })
    };
    let ended = survivor_of(id("delta"), loops);
    assert!(
        ended == id("delta") || ended == id("gamma"),
        "a forwarding that loops ends on one of its rows: {ended}",
    );
}

/// A built-in key for each relation a key can declare, so the one predicate is
/// held to all three at once. No MCP call can declare one of these: the table
/// is the build's own.
const WHO_MAY: [GuardedKey; 4] = [
    GuardedKey {
        key: "only_itself",
        may: MayWrite::Subject,
    },
    GuardedKey {
        key: "only_another",
        may: MayWrite::DifferentIdentity,
    },
    GuardedKey {
        key: "only_above",
        may: MayWrite::Ancestor,
    },
    GuardedKey {
        key: "head_once_held",
        may: MayWrite::HeadOnceHeld { on: "bot:sigma" },
    },
];

fn handle(text: &str) -> EntityId {
    EntityId(text.into())
}

fn writing(key: &str) -> BTreeMap<String, String> {
    [(key.to_string(), "x".to_string())].into()
}

/// **Who may write a key is the key's own declaration, relative to the thing
/// it is written about.** The subject itself, a different identity, or an
/// ancestor on the reports_to chain: one predicate, three relations, and a
/// write from anyone else is refused. Every relation is asked of both of its
/// answers, since a predicate that always refused, or never did, would pass the
/// half that agrees with it.
#[test]
fn a_key_declares_who_may_write_it_relative_to_its_subject() {
    let bot = handle("bot:sigma");
    let other = handle("bot:delta");
    let manager = handle("bot:gamma");
    let above = lineage(&["bot:gamma"], None, &[]);
    let refused = |key: &str, caller: &EntityId, chain: Option<&Lineage>| {
        refuses_unlicensed_write_by(&WHO_MAY, &bot, caller, &writing(key), chain)
    };

    // The subject itself only.
    assert!(refused("only_itself", &bot, None).is_none());
    assert!(refused("only_itself", &other, None).is_some());
    // A different identity only: the relation the existing ceilings have.
    assert!(refused("only_another", &bot, None).is_some());
    assert!(refused("only_another", &other, None).is_none());
    // An ancestor on the chain only: the subject is not on its own chain, a
    // stranger is not, and a manager is.
    assert!(refused("only_above", &manager, Some(&above)).is_none());
    assert!(refused("only_above", &bot, Some(&above)).is_some());
    assert!(refused("only_above", &other, Some(&above)).is_some());
    // **A chain nobody computed refuses**, so a caller that forgot to read the
    // lineage fails closed rather than letting everybody write.
    assert!(refused("only_above", &manager, None).is_some());
    // A key nobody declared is nobody's business.
    assert!(refused("anything_else", &bot, None).is_none());
}

/// **Anybody writes a head-once-held key while it is unheld, and once it is
/// held only the bot that heads the chart does.** Each answer is asked of both
/// callers, so a predicate that always refused, or never did, would fail one.
/// A lineage nobody read refuses, so a caller that forgot to read it fails
/// closed. Held with nobody heading the chart refuses everybody and the refusal
/// says what makes a head.
#[test]
fn a_head_once_held_key_is_free_while_unheld_and_the_heads_once_held() {
    let bot = handle("bot:sigma");
    let other = handle("bot:delta");
    let holds = ["head_once_held".to_string()].into_iter().collect();
    let unheld = Lineage::default();
    let held = Lineage {
        holds,
        ..Default::default()
    };
    let head = Lineage {
        heads: vec![other.clone()],
        ..held.clone()
    };
    let refused = |caller: &EntityId, chain: Option<&Lineage>| {
        refuses_unlicensed_write_by(&WHO_MAY, &bot, caller, &writing("head_once_held"), chain)
    };
    assert!(refused(&other, Some(&unheld)).is_none(), "unheld is free");
    assert!(
        refused(&other, Some(&held)).is_some(),
        "held, caller no head"
    );
    assert!(refused(&other, Some(&head)).is_none(), "held, caller heads");
    assert!(refused(&other, None).is_some(), "no lineage fails closed");
    let said = refused(&other, Some(&held)).expect("refused").to_string();
    assert!(
        said.contains("chart"),
        "the refusal says what makes a head: {said}"
    );
    // The change twin: the fold moving from unheld to held is free, and from
    // held to anything else is not, whether the key changes or goes.
    let before = BTreeMap::new();
    let after = writing("head_once_held");
    let change =
        |before: &BTreeMap<String, String>, after: &BTreeMap<String, String>, chain: &Lineage| {
            refuses_unlicensed_change_by(&WHO_MAY, &bot, &other, before, after, Some(chain))
        };
    assert!(change(&before, &after, &unheld).is_none());
    assert!(change(&after, &before, &held).is_some(), "taking it off");
    assert!(change(&after, &before, &head).is_none());
    // The merge twin: carrying the key onto a record that holds it.
    let merge = |chain: &Lineage| {
        refuses_merge_carrying_by(
            &WHO_MAY,
            &other,
            &bot,
            &handle("bot:gamma"),
            &after,
            Some(chain),
        )
    };
    assert!(merge(&unheld).is_none());
    assert!(merge(&held).is_some());
    assert!(merge(&head).is_none());
}

/// **The refusal names who may make the write** (rule 261): the bots above the
/// subject for an ancestor key, never "the operator" — the operator acts
/// through the assistant, which is above every bot that has a chain.
#[test]
fn a_refused_key_write_names_who_may_make_it() {
    let bot = handle("bot:sigma");
    let above = lineage(&["bot:gamma", "bot:omega"], None, &[]);
    let refused =
        refuses_unlicensed_write_by(&WHO_MAY, &bot, &bot, &writing("only_above"), Some(&above))
            .expect("the subject is not above itself");
    let said = refused.to_string();
    for who in ["bot:gamma", "bot:omega"] {
        assert!(said.contains(who), "{who} may write it: {said}");
    }
    assert!(!said.contains("operator"), "{said}");
    // No chain recorded is said, not left to read as an empty list of people.
    let none = refuses_unlicensed_write_by(
        &WHO_MAY,
        &bot,
        &bot,
        &writing("only_above"),
        Some(&lineage(&[], None, &[])),
    )
    .expect("nobody above");
    assert!(none.to_string().contains("no bot"), "{none}");
}

/// **The same predicate judges an edit's fold and a merge**, so a key that
/// rides on a record about one occasion, or arrives folded in from another
/// thing, is held to the same relation as one written plainly.
#[test]
fn a_change_to_the_fold_and_a_merge_are_judged_by_the_same_relation() {
    let bot = handle("bot:sigma");
    let other = handle("bot:delta");
    let before = BTreeMap::new();
    let after = writing("only_another");
    // The fold changed by a write from the subject itself: refused.
    assert!(refuses_unlicensed_change_by(&WHO_MAY, &bot, &bot, &before, &after, None).is_some());
    // …and from a different identity: not.
    assert!(refuses_unlicensed_change_by(&WHO_MAY, &bot, &other, &before, &after, None).is_none());
    // A fold the write leaves equal is not this guard's business.
    assert!(refuses_unlicensed_change_by(&WHO_MAY, &bot, &bot, &after, &after, None).is_none());
    // A merge that carries the key onto the caller's own bot is refused; onto
    // somebody else's it is not.
    let carried = writing("only_another");
    assert!(
        refuses_merge_carrying_by(
            &WHO_MAY,
            &bot,
            &bot,
            &handle("thing:kettle"),
            &carried,
            None
        )
        .is_some()
    );
    assert!(
        refuses_merge_carrying_by(
            &WHO_MAY,
            &bot,
            &other,
            &handle("thing:kettle"),
            &carried,
            None
        )
        .is_none()
    );
}

fn lineage(above: &[&str], named: Option<&str>, above_named: &[&str]) -> Lineage {
    Lineage {
        above: above.iter().map(|h| handle(h)).collect(),
        named: named.map(handle),
        above_named: above_named.iter().map(|h| handle(h)).collect(),
        ..Default::default()
    }
}

fn reporting_to(manager: &str) -> BTreeMap<String, String> {
    [(REPORTS_TO.to_string(), manager.to_string())].into()
}

/// **A bot cannot be put under one of its own reports, whoever asks.** The
/// assistant is at the top of the chain and has no manager, so the write that
/// names a bot below it as its manager would be allowed by adoption (the bot
/// named is the caller) and would invert the chart. Written before the rule,
/// and refused for the named bot, for the assistant itself and for a bot
/// naming itself.
#[test]
fn a_write_that_puts_a_bot_under_its_own_report_is_refused_whoever_asks() {
    let top = handle("bot:assistant");
    let line = handle("bot:sigma");
    // The line names itself the assistant's manager: it is the bot named, so
    // adoption would let it, and it sits below the assistant.
    let inverted = lineage(&[], Some("bot:sigma"), &["bot:assistant"]);
    for caller in [&line, &top] {
        let refused =
            refuses_unlicensed_write(&top, caller, &reporting_to("bot:sigma"), Some(&inverted))
                .expect("the chart cannot be inverted");
        assert!(
            matches!(refused, MemoryError::ChartCycle { .. }),
            "{caller}: {refused:?}"
        );
    }
    // A bot cannot be its own manager.
    let itself = lineage(&[], Some("bot:sigma"), &[]);
    assert!(matches!(
        refuses_unlicensed_write(&line, &line, &reporting_to("bot:sigma"), Some(&itself)),
        Some(MemoryError::ChartCycle { .. })
    ));
}

/// **The head of a chart places itself, and no other bot places it.** A thing
/// with no manager and reports of its own gets a manager from nobody but
/// itself: not the bot named, not a bot above that one. The cycle check cannot
/// see this, because neither side has a chain. The refusal names the head as the
/// one who may. Paired with the leaf, or a rule that refused every adoption
/// would pass: a thing with no manager and no reports is still adopted by the bot
/// named. A merge that would give the head a manager follows the same rule.
#[test]
fn the_head_of_a_chart_places_itself() {
    let omega = handle("bot:omega");
    let alpha = handle("bot:alpha");
    let head = Lineage {
        has_reports: true,
        ..lineage(&[], Some("bot:omega"), &[])
    };
    for caller in [&omega, &handle("bot:beta")] {
        let refused =
            refuses_unlicensed_write(&alpha, caller, &reporting_to("bot:omega"), Some(&head))
                .expect("no other bot places the head of a chart");
        assert!(
            matches!(
                &refused,
                MemoryError::KeyNotYours { allowed, key, .. }
                    if allowed == &vec![alpha.to_string()] && key == REPORTS_TO
            ),
            "{caller}: {refused:?}"
        );
    }
    assert!(
        refuses_unlicensed_write(&alpha, &alpha, &reporting_to("bot:omega"), Some(&head)).is_none(),
        "the head places itself"
    );
    let leaf = lineage(&[], Some("bot:omega"), &[]);
    assert!(
        refuses_unlicensed_write(
            &handle("bot:gamma"),
            &omega,
            &reporting_to("bot:omega"),
            Some(&leaf)
        )
        .is_none(),
        "a thing with no reports is adopted by the manager it names"
    );

    // A merge that would carry a manager onto the head: refused for a stranger,
    // naming the head, and allowed for the head itself.
    let carried = reporting_to("bot:omega");
    let twin = handle("bot:gamma");
    let refused = refuses_merge_carrying(&omega, &alpha, &twin, &carried, Some(&head))
        .expect("a stranger cannot place the head by merging");
    assert!(
        matches!(
            &refused,
            MemoryError::MergeCarriesGuardedKeys { allowed, .. }
                if allowed == &vec![alpha.to_string()]
        ),
        "{refused:?}"
    );
    assert!(
        refuses_merge_carrying(&alpha, &alpha, &twin, &carried, Some(&head)).is_none(),
        "the head may merge a manager onto itself"
    );
}

/// **Adoption: a bot with no manager takes the one named by that bot or by one
/// above it, and a bot that has one changes it only through its chain.** Every
/// branch is asked of both of its answers, since a rule that refused
/// everything, or allowed everything, would pass the half that agrees.
#[test]
fn a_first_manager_is_adopted_and_a_changed_one_stays_with_the_chain() {
    let orphan = handle("bot:sigma");
    let named = "bot:gamma";
    let adopts = |caller: &str, above_named: &[&str]| {
        refuses_unlicensed_write(
            &orphan,
            &handle(caller),
            &reporting_to(named),
            Some(&lineage(&[], Some(named), above_named)),
        )
    };
    // The bot named adopts its report; a bot above it may place one under it.
    assert!(adopts("bot:gamma", &["bot:omega"]).is_none());
    assert!(adopts("bot:omega", &["bot:omega"]).is_none());
    // A stranger, and the orphan naming its own boss, are refused.
    assert!(adopts("bot:delta", &["bot:omega"]).is_some());
    assert!(adopts("bot:sigma", &["bot:omega"]).is_some());

    // With a manager already, only the chain changes it.
    let managed = |caller: &str| {
        refuses_unlicensed_write(
            &orphan,
            &handle(caller),
            &reporting_to(named),
            Some(&lineage(&["bot:rho", "bot:omega"], Some(named), &[])),
        )
    };
    assert!(managed("bot:rho").is_none());
    assert!(managed("bot:omega").is_none());
    // The bot being named is not on the chain, so it cannot take a report
    // away from its manager; and a stranger cannot either.
    assert!(managed("bot:gamma").is_some());
    assert!(managed("bot:delta").is_some());
    assert!(managed("bot:sigma").is_some());

    // Clearing the key is a change to the chart too.
    let cleared = |caller: &str| {
        refuses_unlicensed_write(
            &orphan,
            &handle(caller),
            &BTreeMap::from([(REPORTS_TO.to_string(), String::new())]),
            Some(&lineage(&["bot:rho"], None, &[])),
        )
    };
    assert!(cleared("bot:rho").is_none());
    assert!(cleared("bot:delta").is_some());
}

/// **The refusal names the bots that may make the write**, never "the
/// operator": the operator acts through the assistant, which is above every bot
/// that has a chain.
#[test]
fn a_refused_chart_write_names_the_bots_that_may_make_it() {
    let orphan = handle("bot:sigma");
    let stranger = handle("bot:delta");
    // A bot with a manager: its chain.
    let said = refuses_unlicensed_write(
        &orphan,
        &stranger,
        &reporting_to("bot:gamma"),
        Some(&lineage(&["bot:rho", "bot:omega"], Some("bot:gamma"), &[])),
    )
    .expect("a stranger may not change it")
    .to_string();
    for who in ["bot:rho", "bot:omega"] {
        assert!(said.contains(who), "{who} may: {said}");
    }
    // A bot without one: the bot being named, and the bots above it.
    let said = refuses_unlicensed_write(
        &orphan,
        &stranger,
        &reporting_to("bot:gamma"),
        Some(&lineage(&[], Some("bot:gamma"), &["bot:omega"])),
    )
    .expect("a stranger may not adopt")
    .to_string();
    for who in ["bot:gamma", "bot:omega"] {
        assert!(said.contains(who), "{who} may: {said}");
    }
    assert!(!said.contains("operator"), "{said}");
}

/// **A walk up the chart stops where a chart can stop**: at a bot with no
/// manager, at a value that is no handle, at a loop and at the depth bound. A
/// chart that looped, or ran for ever, would hang the write that asks. Every
/// stop is paired with a walk that does continue, since a walk that always
/// stopped at once would pass the stops alone.
#[test]
fn a_walk_up_the_chart_stops_at_the_top_a_loop_a_non_handle_and_the_depth_bound() {
    // Reading a handle needs the kinds a booted store loads.
    let _booted = InMemoryMemory::booted();
    let walk = |start: &str, managers: &dyn Fn(&str) -> Option<String>| {
        let mut walk = ChainWalk::from(&handle(start));
        let mut reads = 0;
        while let Some(at) = walk.at().cloned() {
            // A walk that never stops must fail here and not hang the suite.
            reads += 1;
            assert!(reads < 10 * MAX_CHAIN, "the walk does not stop");
            walk.step(managers(at.as_str()).as_deref());
        }
        walk.above()
    };
    // A chain that continues: a to b to c, and c has none.
    let above = walk("bot:delta", &|at| match at {
        "bot:delta" => Some("bot:gamma".into()),
        "bot:gamma" => Some("bot:omega".into()),
        _ => None,
    });
    assert_eq!(above, vec![handle("bot:gamma"), handle("bot:omega")]);
    // A loop: a to b to a stops at b, and the start is not its own manager.
    let looped = walk("bot:delta", &|at| match at {
        "bot:delta" => Some("bot:gamma".into()),
        _ => Some("bot:delta".into()),
    });
    assert_eq!(looped, vec![handle("bot:gamma")]);
    // A value that is no handle ends the walk without a manager.
    let prose = walk("bot:delta", &|_| Some("whoever is free".into()));
    assert!(prose.is_empty(), "{prose:?}");
    // A chart longer than the bound stops at it.
    let endless = walk("bot:delta", &|at| {
        let n: usize = at
            .rsplit('-')
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        // Built at run time: the roster scan reads only handles in the source.
        let slug = format!("link-{}", n + 1);
        Some(format!("bot:{slug}"))
    });
    assert!(endless.len() <= MAX_CHAIN + 1, "{}", endless.len());
    assert!(endless.len() > 1);
}

/// **The four keys a bot cannot write about itself are declarations in the
/// one table**, each a different-identity key, so moving them changed nothing a
/// caller sees.
#[test]
fn the_four_existing_keys_are_different_identity_keys() {
    for key in [THOUGHT_CAPACITY, THOUGHT_BODY_CAP, RULE_SEATS, CLAIMS_ROLE] {
        let rule = GUARDED_KEYS
            .iter()
            .find(|rule| rule.key == key)
            .unwrap_or_else(|| panic!("{key} is declared"));
        assert_eq!(rule.may, MayWrite::DifferentIdentity, "{key}");
    }
    // …and the chart's own key is the one superior-only key beside them.
    let chart = GUARDED_KEYS
        .iter()
        .find(|rule| rule.key == REPORTS_TO)
        .expect("reports_to is declared");
    assert_eq!(chart.may, MayWrite::Superior);
    // …and the operator's key is the one head-once-held key. The spelling is
    // stored in a fields bag and nothing outside this process declares it, so
    // it is pinned as a literal.
    let operator = GUARDED_KEYS
        .iter()
        .find(|rule| rule.key == "operator")
        .expect("operator is declared");
    assert_eq!(
        operator.may,
        MayWrite::HeadOnceHeld {
            on: "topic:instance"
        }
    );
    assert_eq!(OPERATOR, "operator");
    assert_eq!(INSTANCE_RECORD, "topic:instance");
    assert_eq!(GUARDED_KEYS.len(), 7);
}

/// **The cutoff counts back the number of runs it is given**, not a fixed
/// twenty: three runs age on a setting of three and answer the oldest of them,
/// and the same three runs answer nothing on the default.
#[test]
fn the_cutoff_counts_back_the_number_of_runs_it_is_given() {
    let starts: Vec<jiff::Timestamp> = (0..3)
        .map(|i| at(&format!("2026-04-{:02}T00:00:00Z", i + 1)))
        .collect();
    assert_eq!(aging_cutoff(&starts, 3), Some(starts[0]), "{starts:?}");
    assert_eq!(aging_cutoff(&starts, 2), Some(starts[1]), "{starts:?}");
    assert_eq!(aging_cutoff(&starts, AGES_AFTER_RUNS), None, "{starts:?}");
}

/// **A setting that is not a whole number above zero reads as no setting**,
/// so a mistyped value never ages every thought at once. A good one is read as
/// written, trimmed.
#[test]
fn the_ageing_setting_reads_a_good_value_and_falls_back_on_a_bad_one() {
    let fold = |value: Option<&str>| -> BTreeMap<String, String> {
        value
            .map(|v| ("thought_ages_after_runs".to_string(), v.to_string()))
            .into_iter()
            .collect()
    };
    assert_eq!(ages_after_runs_of(&fold(Some("3"))), 3);
    assert_eq!(ages_after_runs_of(&fold(Some(" 7 "))), 7);
    assert_eq!(ages_after_runs_of(&fold(None)), AGES_AFTER_RUNS);
    for bad in ["0", "", "soon", "-2", "2.5"] {
        assert_eq!(
            ages_after_runs_of(&fold(Some(bad))),
            AGES_AFTER_RUNS,
            "{bad:?} is not a setting"
        );
    }
}

/// **The ageing setting is a different-identity key beside the capacity it
/// is read with.** Pinned by its stored spelling, which nothing outside this
/// process declares.
#[test]
fn the_ageing_setting_is_a_different_identity_key() {
    let rule = GUARDED_KEYS
        .iter()
        .find(|rule| rule.key == "thought_ages_after_runs")
        .expect("thought_ages_after_runs is declared");
    assert_eq!(rule.may, MayWrite::DifferentIdentity);
}

fn thought(id: &str, pointer: &str) -> Fact {
    Fact {
        id: FactId(id.into()),
        home: EntityId("bot:contract-thought-capacity".into()),
        subject: EntityId("bot:contract-thought-capacity".into()),
        content: format!("a thought pointing at {pointer}"),
        details: None,
        provenance: Provenance::Inference,
        standing: Standing::Open,
        status: FactStatus::Active,
        recorded_at: Date::constant(2026, 1, 1),
        happened_at: None,
        happened_through: None,
        edge: Some(Edge::new(EdgeShape::Connection, EntityId(pointer.into()))),
        fields: Default::default(),
        refs: Vec::new(),
        derived_from: None,
        stands_for: Vec::new(),
        inserted_at: None,
        stale_after: None,
    }
}

/// 🚨 **Fewer runs than the threshold asks no question at all** — a
/// true absence looks exactly like this: `None`, not a cutoff computed
/// from whatever runs do exist. A cutoff answered from too few runs
/// would age a thought no bot has had the chance to even reach.
#[test]
fn fewer_runs_than_the_threshold_answers_no_cutoff() {
    let starts: Vec<_> = (0..AGES_AFTER_RUNS - 1)
        .map(|_| at("2026-01-01T00:00:00Z"))
        .collect();
    assert_eq!(aging_cutoff(&starts, AGES_AFTER_RUNS), None, "{starts:?}");
}

/// **Exactly the threshold answers the oldest of them** — the run that,
/// counting back from today, is the [`AGES_AFTER_RUNS`]th.
#[test]
fn exactly_the_threshold_answers_the_oldest_run() {
    let starts: Vec<jiff::Timestamp> = (0..AGES_AFTER_RUNS)
        .map(|i| at(&format!("2026-01-{:02}T00:00:00Z", i + 1)))
        .collect();
    let oldest = starts[0];
    assert_eq!(
        aging_cutoff(&starts, AGES_AFTER_RUNS),
        Some(oldest),
        "{starts:?}"
    );
}

/// 🚨 **More than the threshold answers the Nth-newest, never the
/// oldest of all of them** — extra runs beyond the window do not push
/// the cutoff back further, or a bot that has run for years would never
/// age anything.
#[test]
fn more_than_the_threshold_answers_the_nth_newest_not_the_oldest() {
    let extra = 5;
    let starts: Vec<jiff::Timestamp> = (0..AGES_AFTER_RUNS + extra)
        .map(|i| at(&format!("2026-02-{:02}T00:00:00Z", i + 1)))
        .collect();
    let nth_newest = starts[extra];
    let oldest = starts[0];
    let cutoff = aging_cutoff(&starts, AGES_AFTER_RUNS).expect("more than enough runs");
    assert_eq!(cutoff, nth_newest, "{starts:?}");
    assert_ne!(
        cutoff, oldest,
        "the extra runs must not push the cutoff back: {starts:?}"
    );
}

/// 🚨 **A thought touched before the cutoff is aged out, and one touched
/// at or after it stays live** — the boundary itself counts as live, so
/// a run that touches something in its own window never ages it.
#[test]
fn split_by_age_separates_on_the_cutoff_boundary() {
    let cutoff = at("2026-03-10T00:00:00Z");
    let old = thought("f1", "thing:contract-thought-pointer-v1");
    let boundary = thought("f2", "thing:contract-thought-pointer-v2");
    let touched = std::collections::HashMap::from([
        (old.id.clone(), at("2026-03-09T23:59:59Z")),
        (boundary.id.clone(), cutoff),
    ]);
    let split = split_by_age(vec![old.clone(), boundary.clone()], &touched, Some(cutoff));
    assert_eq!(split.aged_out, vec![old], "{split:?}");
    assert_eq!(split.live, vec![boundary], "{split:?}");
}

/// 🚨 **A thought nobody ever recorded a touch for is never aged** —
/// absence in `touched` is not evidence of age, and treating it as one
/// would age exactly the rows a write predating this feature left
/// behind.
#[test]
fn a_thought_with_no_recorded_touch_stays_live() {
    let cutoff = at("2026-03-10T00:00:00Z");
    let untouched = thought("f1", "thing:contract-thought-pointer-v1");
    let split = split_by_age(
        vec![untouched.clone()],
        &std::collections::HashMap::new(),
        Some(cutoff),
    );
    assert_eq!(split.live, vec![untouched], "{split:?}");
    assert!(split.aged_out.is_empty(), "{split:?}");
}

/// **No cutoff ages nothing at all** — the shape every caller gets
/// before a bot has run enough times to ask the question.
#[test]
fn no_cutoff_ages_nothing() {
    let f = thought("f1", "thing:contract-thought-pointer-v1");
    let touched = std::collections::HashMap::from([(f.id.clone(), at("2020-01-01T00:00:00Z"))]);
    let split = split_by_age(vec![f.clone()], &touched, None);
    assert_eq!(split.live, vec![f]);
    assert!(split.aged_out.is_empty());
}

/// 🚨 **A refusal names the way forward.** `validate_field` backs a
/// name, a source, an alias, a crm value and an archive reason — one
/// shared shape, so one case here covers every caller of it. A caller
/// told only "too long" has no idea what "fixed" means.
#[test]
fn a_frontmatter_field_past_the_limit_is_told_the_limit() {
    assert!(validate_field("name", &"x".repeat(200)).is_ok());
    let refused = validate_field("name", &"x".repeat(201)).expect_err("and it is capped");
    let said = refused.to_string();
    assert!(
        said.contains("200"),
        "the refusal must name the limit a caller has to write under: {said}"
    );
}

/// 🚨 **Both halves of a stale handle, in one case**: a rename it survives,
/// and a handle that never existed, which must stay a miss.
///
/// A resolver that answered every handle would pass on the first half
/// alone; one that answered none would pass on the second.
#[test]
fn resolve_handle_follows_a_rename_and_a_never_existed_handle_still_misses() {
    let _booted = InMemoryMemory::booted();
    let survivor = thing("person:milhouse-2", Some("k7h2mn"));
    let known = [survivor.clone()];
    let former = [FormerHandle {
        former: EntityId("person:milhouse".into()),
        badge: "k7h2mn".into(),
        changed_at: Date::constant(2026, 3, 1),
    }];

    assert_eq!(
        resolve_handle(&EntityId("person:milhouse".into()), &known, &former),
        Some(&survivor),
        "a handle that renamed must still resolve to whoever wears its badge now",
    );
    assert_eq!(
        resolve_handle(&EntityId("person:zzz".into()), &known, &former),
        None,
        "a handle nothing ever answered to must stay a miss",
    );
}

/// One write of a key, at the place in that key's history it took.
fn wrote(key: &str, ordinal: u64, value: Option<&str>) -> KeyWrite {
    KeyWrite {
        provenance: Provenance::Inference,
        standing: Standing::Open,
        note: None,
        key: key.to_string(),
        ordinal,
        value: value.map(str::to_string),
        fact: FactId("f1".into()),
        status: FactStatus::Active,
    }
}

/// **A field key has a length, and the domain is what says so.**
///
/// The column holding a key is 191 characters and a caller can write more,
/// so without a limit here the store is the thing deciding where a key
/// stops working: a short key round-trips and a long one comes back as a
/// store failure, which is a caller mistake wearing a broken-server answer
/// (rules 9 and 68).
///
/// **Both ends in one case.** A limit that refused everything would pass a
/// check that only sent the long key, and a limit that refused nothing
/// would pass a check that only sent the short one. The refusal has to say
/// the number, because a caller that cannot read the limit off the answer
/// finds it by bisection.
#[test]
fn a_field_key_is_bounded_by_the_domain_and_the_refusal_says_the_bound() {
    let at_the_limit = BTreeMap::from([("k".repeat(MAX_KEY_CHARS), "value".to_string())]);
    validate_fields(&at_the_limit).expect("a key at the limit is one a caller may write");

    let over = BTreeMap::from([("k".repeat(MAX_KEY_CHARS + 1), "value".to_string())]);
    let refused = validate_fields(&over)
        .expect_err("a key past the limit is refused before it reaches a store");
    let said = refused.to_string();
    assert!(
        said.contains(&MAX_KEY_CHARS.to_string()),
        "the refusal must carry the limit a caller has to write under: {said}"
    );
}

/// **A key declared a counter sums its writes; every other key still takes
/// the newest.**
///
/// Both halves in one case. A fold that summed every key would pass a check
/// that only looked at the counter, and a fold that summed nothing would
/// pass a check that only looked at the other key.
///
/// The last read is the negative the whole feature rests on: the same
/// writes, with nothing declared, fold the way they always have. Without it
/// the case passes on a build that sums every number it sees, and the
/// declaration would be buying nothing.
#[test]
fn a_counter_sums_its_writes_and_every_other_key_takes_the_newest() {
    let writes = vec![
        wrote("donuts", 1, Some("1")),
        wrote("donuts", 2, Some("1")),
        wrote("donuts", 3, Some("1")),
        wrote("mood", 1, Some("hungry")),
        wrote("mood", 2, Some("content")),
    ];
    let snacking = types::DeclaredType::new(
        "snacking",
        vec![
            types::Field::summing("donuts"),
            types::Field::new("mood", types::ValueType::Text),
        ],
    );

    let folded = folded_fields(&writes, std::slice::from_ref(&snacking));
    assert_eq!(
        folded.get("donuts").map(String::as_str),
        Some("3"),
        "three writes of one is three: {folded:?}",
    );
    assert_eq!(
        folded.get("mood").map(String::as_str),
        Some("content"),
        "a key nobody declared a counter reads back its newest write: {folded:?}",
    );

    let undeclared = folded_fields(&writes, &[]);
    assert_eq!(
        undeclared.get("donuts").map(String::as_str),
        Some("1"),
        "with nothing declared the same writes take the newest, which is what the \
             declaration changes: {undeclared:?}",
    );
}

/// **A key declared to describe its record never folds onto the thing; every
/// other key still takes the newest.** Both halves in one case: a fold that
/// dropped every key would pass a check that only looked at the described one,
/// and one that dropped none would pass a check that only looked at the other.
///
/// The last read is the negative the whole behaviour rests on: the same writes
/// with nothing declared fold the way they always have, so the declaration is
/// what changes them (rule 360: the build's vocabulary attaches behaviour, a
/// key nobody declared does not).
#[test]
fn a_key_that_describes_its_record_never_folds_and_every_other_key_still_does() {
    let writes = vec![
        wrote("starred", 1, Some("true")),
        wrote("subject", 1, Some("boot: claim and wake")),
        wrote("mood", 1, Some("hungry")),
        wrote("mood", 2, Some("content")),
    ];
    let labels = types::DeclaredType::shipped(
        "record-labels",
        vec![
            types::Field::describing("starred"),
            types::Field::describing("subject"),
        ],
    );

    let folded = folded_fields(&writes, std::slice::from_ref(&labels));
    assert!(
        !folded.contains_key("starred") && !folded.contains_key("subject"),
        "a label on one record is not a property of the thing: {folded:?}",
    );
    assert_eq!(
        folded.get("mood").map(String::as_str),
        Some("content"),
        "a key nobody declared folds as it always did: {folded:?}",
    );

    let undeclared = folded_fields(&writes, &[]);
    assert_eq!(
        undeclared.get("starred").map(String::as_str),
        Some("true"),
        "with nothing declared the same writes fold, which is what the declaration \
         changes: {undeclared:?}",
    );

    // Nothing wins a key that never folds, so there is no backing to name.
    let backing = folded_backing(&writes, std::slice::from_ref(&labels));
    assert!(!backing.contains_key("starred"), "{backing:?}");
    assert!(backing.contains_key("mood"), "{backing:?}");
}

/// **Only the build declares a key that describes its record** (rule 360). A
/// type a caller declares is refused over it, and the same field in a shipped
/// type is taken: a bot that could declare it could make any key stop folding
/// for everybody. A described key is optional by nature, so one declared
/// required is refused too, since no thing could ever hold it.
#[test]
fn only_a_shipped_type_may_declare_a_key_that_describes_its_record() {
    let fields = || vec![types::Field::describing("starred")];
    assert!(types::validate_type(&types::DeclaredType::shipped("labels", fields())).is_ok());
    let refused = types::validate_type(&types::DeclaredType::new("labels", fields()))
        .expect_err("a caller does not attach behaviour to a key");
    assert!(
        matches!(refused, MemoryError::InvalidType(_)),
        "{refused:?}"
    );
    let required =
        types::DeclaredType::shipped("labels", vec![types::Field::describing("starred").needed()]);
    assert!(
        types::validate_type(&required).is_err(),
        "a key no thing can hold is not one to require"
    );
}

/// **The token round-trips**, because the store keeps a declaration's fold as
/// text and reads it back.
#[test]
fn a_fold_that_describes_reads_back_from_its_token() {
    assert_eq!(
        types::Fold::of_token(types::Fold::Describes.as_token()),
        Some(types::Fold::Describes)
    );
    // A counter declared beside it keeps its behaviour: the more specific one
    // wins when two types name one key.
    let both = [
        types::DeclaredType::shipped("a", vec![types::Field::describing("k")]),
        types::DeclaredType::new("b", vec![types::Field::summing("k")]),
    ];
    assert_eq!(types::fold_of("k", &both), types::Fold::Sum);
    assert_eq!(types::fold_of("k", &both[..1]), types::Fold::Describes);
}

/// **A clear ends a total, and the writes after it start a new one.**
///
/// A clear is a write and takes the key off the thing. On a counter that
/// has to reset it, or a key somebody cleared would keep counting from
/// whatever it held before nobody held it.
#[test]
fn clearing_a_counter_ends_its_total_and_the_next_write_starts_over() {
    let snacking = types::DeclaredType::new("snacking", vec![types::Field::summing("donuts")]);
    let cleared = folded_fields(
        &[
            wrote("donuts", 1, Some("2")),
            wrote("donuts", 2, Some("3")),
            wrote("donuts", 3, None),
        ],
        std::slice::from_ref(&snacking),
    );
    assert_eq!(
        cleared.get("donuts"),
        None,
        "a clear takes a counter off the thing exactly as it takes any key off: {cleared:?}",
    );

    // **Two writes after the clear, so the three answers are three different
    // numbers.** Counting since the clear is five; counting every write is
    // ten; taking the newest is four. One write after the clear would leave
    // the first and the third indistinguishable, and the case would pass on
    // a build that sums nothing at all.
    let again = folded_fields(
        &[
            wrote("donuts", 1, Some("2")),
            wrote("donuts", 2, Some("3")),
            wrote("donuts", 3, None),
            wrote("donuts", 4, Some("1")),
            wrote("donuts", 5, Some("4")),
        ],
        std::slice::from_ref(&snacking),
    );
    assert_eq!(
        again.get("donuts").map(String::as_str),
        Some("5"),
        "the total after a clear counts the writes since it, not the five before: {again:?}",
    );
}

/// **Only the writes that count are counted.**
///
/// A write carried by a record somebody took back is not what the thing is
/// now, and the rule already holds for newest-wins. A sum that added every
/// row would resurrect the retracted one as arithmetic, where the old fold
/// merely passed it over.
#[test]
fn a_counter_passes_over_a_write_whose_record_no_longer_counts() {
    let snacking = types::DeclaredType::new("snacking", vec![types::Field::summing("donuts")]);
    let folded = folded_fields(
        &[
            wrote("donuts", 1, Some("1")),
            KeyWrite {
                status: FactStatus::Archived,
                ..wrote("donuts", 2, Some("40"))
            },
            wrote("donuts", 3, Some("1")),
        ],
        std::slice::from_ref(&snacking),
    );
    assert_eq!(
        folded.get("donuts").map(String::as_str),
        Some("2"),
        "the retracted forty is passed over and the two standing ones add: {folded:?}",
    );
}

/// The invariant, red→green, in milliseconds against the fake: a capture
/// succeeds only if a subsequent recall returns the fact.
#[tokio::test]
async fn capture_reads_back_against_the_fake() {
    contract::capture_reads_back(&InMemoryMemory::booted()).await;
}

/// The session rule for rewriting testimony, in milliseconds against the fake.
#[tokio::test]
async fn a_rewrite_of_testimony_belongs_to_its_session_against_the_fake() {
    contract::a_rewrite_of_testimony_belongs_to_the_session_that_wrote_it(&InMemoryMemory::booted())
        .await;
}

/// The setting bag and the one-class rule, in milliseconds against the fake.
#[tokio::test]
async fn a_setting_bag_reaches_the_thing_against_the_fake() {
    contract::a_setting_bag_reaches_the_thing_and_a_key_stays_in_its_bag(&InMemoryMemory::booted())
        .await;
}

/// 🚨 **The three class tokens, pinned as literals.** They are stored in a column
/// and nothing outside this process declares them, so an assertion built from the
/// constants would move with them and pin nothing.
#[test]
fn the_three_write_classes_are_stored_as_legacy_own_and_sets() {
    use crate::memory::WriteClass;
    assert_eq!(WriteClass::Legacy.as_token(), "legacy");
    assert_eq!(WriteClass::Own.as_token(), "own");
    assert_eq!(WriteClass::Sets.as_token(), "sets");
    for class in [WriteClass::Legacy, WriteClass::Own, WriteClass::Sets] {
        assert_eq!(WriteClass::from_token(class.as_token()), class);
        assert!(
            crate::memory::reaches_the_thing(class),
            "every class reaches its thing today",
        );
    }
    assert_eq!(
        WriteClass::from_token("something-newer"),
        WriteClass::Legacy,
        "a token this build does not know reads as the class every write once had",
    );
}

/// An edit and its claim in one act, in milliseconds against the fake.
#[tokio::test]
async fn an_entity_edited_with_a_claim_is_whole_against_the_fake() {
    contract::an_entity_edited_with_a_claim_is_made_whole_or_not_at_all(&InMemoryMemory::booted())
        .await;
}

/// The full behavioural contract holds for the fake — the same suite the
/// real-store test runs against the real adapter.
#[tokio::test]
async fn fake_satisfies_the_contract() {
    contract::run_all(&InMemoryMemory::booted()).await;
}

/// **The contract dealt into parts runs every case once, and each part holds on
/// a store of its own.** A real store runs the parts side by side, one store
/// each. The parts' counts must add up to the whole list's, or a case was
/// dropped between them. Every part must also have something to run, or a part
/// is a store started for nothing. A part that leans on a case outside it
/// fails here, on a store its own cases alone have filled.
#[tokio::test]
async fn the_parts_of_the_contract_run_every_case_once_each_on_its_own_store() {
    let total = contract::case_count();
    let mut ran = Vec::new();
    for part in 0..contract::PARTS {
        let store = InMemoryMemory::booted();
        ran.push(contract::run_part(&store, part, contract::PARTS).await);
    }
    assert!(ran.iter().all(|n| *n > 0), "a part ran nothing: {ran:?}");
    assert_eq!(ran.iter().sum::<usize>(), total, "parts: {ran:?}");
}

/// **Every case of the contract holds alone, on a store of its own.** A part is
/// a run of neighbouring cases on a fresh store, so a case that leans on a
/// record some other case wrote passes the run of the whole list and fails as
/// soon as a part is cut between the two. Running each case by itself is the
/// strictest cut there is: every case held here holds in any part.
#[tokio::test]
async fn every_case_of_the_contract_holds_alone_on_a_store_of_its_own() {
    let total = contract::case_count();
    let mut failing = Vec::new();
    for case in 0..total {
        let ran = tokio::spawn(async move {
            contract::run_part(&InMemoryMemory::booted(), case, total).await
        })
        .await;
        match ran {
            Ok(1) => {}
            Ok(other) => failing.push(format!("case {case}: ran {other} cases, not one")),
            Err(panic) => failing.push(format!("case {case}: {panic}")),
        }
    }
    assert!(
        failing.is_empty(),
        "cases that need another case: {failing:#?}"
    );
}

/// **A handle stored as plain text under a key nobody declared, from before ids
/// were kept, still follows its target through a rename in the fake**, as the
/// real store's reads do: the fake lies where the real store lies, and tells the
/// truth where it tells it.
#[tokio::test]
async fn a_plain_text_handle_under_an_undeclared_key_resolves_through_a_rename_in_the_fake() {
    let memory = InMemoryMemory::booted();
    let was = EntityId("work:contract-follows-was".into());
    let now = EntityId("work:contract-follows-relocated".into());
    let holder = EntityId("thing:contract-follows-holder".into());
    for (id, name) in [(&was, "Original"), (&holder, "Holder")] {
        memory
            .add_entity(NewEntity::new(id.clone(), name, "test"))
            .await
            .expect("add ok")
            .written()
            .expect("nothing collides");
    }
    memory.fields_past_the_guard(&holder, &[("blocks", was.as_str())]);
    memory
        .rename_entity(&was, &now, None, jiff::civil::date(2026, 8, 2), None)
        .await
        .expect("the rename lands")
        .written()
        .expect("nothing collides");

    let fields = memory.fields(&holder).await.expect("fields reads");
    assert_eq!(fields["blocks"], now.to_string(), "{fields:?}");
}

/// A role's exclusivity against the fake — the same suite the gated
/// integration test runs against real Dolt.
#[tokio::test]
async fn fake_satisfies_the_role_claim_contract() {
    contract::run_all_role_claims(&InMemoryMemory::booted()).await;
}

/// **A known defect, run on purpose** (`cargo test -- --ignored`): a deadline
/// on a claim about a person makes the person owed. The case states the right
/// answer, so it is red until the fix lands; the real store runs the same case.
#[tokio::test]
#[ignore = "known defect: card 1903, decision log 340, the fix's shape is the operator's"]
async fn fake_does_not_owe_a_person_for_a_deadline_on_one_claim() {
    contract::a_deadline_on_a_claim_about_a_person_does_not_make_the_person_owed(
        &InMemoryMemory::booted(),
    )
    .await;
}

/// The mark's own contract against the fake — the same suite the gated
/// integration test runs against real Dolt.
#[tokio::test]
async fn fake_satisfies_the_stands_for_contract() {
    contract::run_all_stands_for(&InMemoryMemory::booted()).await;
}

/// **The mention contract against the fake**, over the layer that resolves
/// and renders and over the same store read bare.
///
/// The two handles address one store: the claim is that what is KEPT and
/// what is SERVED differ, and a case holding only one of them cannot make
/// it.
#[tokio::test]
async fn the_fake_stores_a_mention_as_a_badge_and_serves_it_as_a_handle() {
    let store = std::sync::Arc::new(InMemoryMemory::booted());
    let bare: std::sync::Arc<dyn Memory> = store.clone();
    contract::run_all_mentioning(
        &mention::Mentioning::new(bare.clone()),
        &*bare,
        &contract::FakeRehandles(store),
    )
    .await;
}

/// **Asking who points at a thing costs the same however many entities and
/// however many claims the store holds, in the reads it repeats.** The trait's
/// default asks `recall` of every entity, and the fake's `recall` rebuilds the
/// whole row list each time, so a recall of a whole kind, which asks this once
/// per object, cost the cube of the store: 150 things took seconds and 400 took
/// more than half a minute. Serving each claim then cloned every declared type
/// and passed over every write, once per claim, so the cost still moved with
/// the claims.
///
/// **The reads counted are the ones that rebuild or clone the whole of
/// something**: the entity rows, the declared types and the list of writes.
/// They are asked of stores that differ in one thing at a time, entities and
/// then claims, so a repeat that moves with either fails. The positive comes
/// first: the one record that names the target is the one that comes back, and
/// a claim naming something else does not, so a fake that read less because it
/// answered nothing, or everything, does not pass.
#[tokio::test]
async fn the_fake_answers_who_points_at_a_thing_without_repeating_whole_reads_per_entity_or_claim()
{
    /// What one question over a store of this size cost, as repeated whole reads.
    async fn reads_for_one_question(fillers: usize, extra_claims: usize) -> (usize, usize, usize) {
        let store = InMemoryMemory::booted();
        let target = EntityId("event:contract-winter-fest".into());
        let holder = EntityId("person:contract-milhouse".into());
        for (id, name) in [
            (&target, "Contract Winter Fest"),
            (&holder, "Contract Milhouse"),
        ] {
            store
                .add_entity(NewEntity::new(id.clone(), name, "fixture"))
                .await
                .expect("the fixture is written");
        }
        // Unrelated words, because the creation guard refuses a name that
        // contains or nearly matches another, and a refused fixture would
        // shrink the store without a word.
        let mut seed = 0x9E37_79B9_7F4A_7C15_u64;
        for _ in 0..fillers {
            let word: String = (0..10)
                .map(|_| {
                    seed = seed
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    (b'a' + ((seed >> 33) % 26) as u8) as char
                })
                .collect();
            store
                .add_entity(NewEntity::new(
                    EntityId(format!("thing:{word}")),
                    word,
                    "fixture",
                ))
                .await
                .expect("the fixture is written")
                .written()
                .expect("the guard did not refuse the fixture");
        }
        assert_eq!(
            store.list_entities(None).await.expect("a listing").len(),
            2 + fillers,
            "the store holds every entity the case wrote"
        );
        let held = store
            .capture(NewFact {
                fields: [("admits".to_string(), target.to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    holder.clone(),
                    "holds a full pass",
                    jiff::civil::date(2026, 8, 1),
                )
            })
            .await
            .expect("a capture")
            .written()
            .expect("nothing blocks it");
        // Claims naming something else, which must not come back. Each names the
        // holder itself, a handle, so serving one has a handle to resolve.
        for n in 0..extra_claims {
            store
                .capture(NewFact {
                    fields: [("admits".to_string(), holder.to_string())]
                        .into_iter()
                        .collect(),
                    ..NewFact::about(
                        holder.clone(),
                        format!("holds pass number {n} to itself"),
                        jiff::civil::date(2026, 8, 2),
                    )
                })
                .await
                .expect("a capture")
                .written()
                .expect("nothing blocks it");
        }

        let (rows, declared, scans) = (
            store.index_reads(),
            store.declaration_reads(),
            store.write_scans(),
        );
        let pointing = store.referring_to(&target).await.expect("who points here");
        assert_eq!(
            pointing
                .iter()
                .map(|f| f.address().to_string())
                .collect::<Vec<_>>(),
            vec![held.address().to_string()],
            "the record naming the target comes back, and the ones naming something else do not"
        );
        (
            store.index_reads() - rows,
            store.declaration_reads() - declared,
            store.write_scans() - scans,
        )
    }

    let few_entities = reads_for_one_question(20, 2).await;
    let many_entities = reads_for_one_question(60, 2).await;
    assert_eq!(
        few_entities, many_entities,
        "(rows rebuilt, declared types cloned, write passes) over 22 entities and over 62: the \
         cost of one question moves with the entities"
    );
    let few_claims = reads_for_one_question(20, 2).await;
    let many_claims = reads_for_one_question(20, 12).await;
    assert_eq!(
        few_claims, many_claims,
        "(rows rebuilt, declared types cloned, write passes) over 3 claims and over 13: the cost \
         of one question moves with the claims"
    );
}

/// A store wired with a supplied record, for the two creation-guard specs
/// below that need one — `run_all` above never wires one, so these run on
/// their own.
fn fake_knowing_a_supplied_view() -> InMemoryMemory {
    InMemoryMemory::booted().knowing(owned::Provisions::new(vec![owned::Provision::record(
        Entity {
            id: EntityId(contract::SUPPLIED_VIEW_FOR_THE_GUARD_SPECS.into()),
            kind: EntityKind::VIEW,
            name: "Contract Shipped View".into(),
            aliases: Vec::new(),
            source: "jojobot".into(),
            crm: None,
            parent: None,
            boot: Default::default(),
            merged_into: None,
            badge: None,
            archived: None,
        },
        BTreeMap::new(),
    )]))
}

#[tokio::test]
async fn add_entity_guards_hold_for_stored_and_supplied_against_the_fake() {
    contract::add_entity_guards_hold_for_stored_and_supplied(
        &InMemoryMemory::booted(),
        &fake_knowing_a_supplied_view(),
    )
    .await;
}

#[tokio::test]
async fn a_claim_on_a_supplied_record_reads_back_against_the_fake() {
    contract::a_claim_on_a_supplied_record_reads_back(&fake_knowing_a_supplied_view()).await;
}

/// **Every entity read, counted in one place, against the fake.**
#[tokio::test]
async fn every_entity_read_answers_for_a_supplied_record_against_the_fake() {
    contract::every_entity_read_answers_for_a_supplied_record(&fake_knowing_a_supplied_view())
        .await;
}

/// **Every existence-gated read answers alike, stored and supplied, against
/// the fake.**
#[tokio::test]
async fn a_captured_claim_reads_through_every_gated_read_stored_and_supplied_against_the_fake() {
    contract::a_captured_claim_reads_through_every_gated_read_stored_and_supplied(
        &InMemoryMemory::booted(),
        &fake_knowing_a_supplied_view(),
    )
    .await;
}

/// **An edit on a claim lands alike, stored and supplied, against the
/// fake.**
#[tokio::test]
async fn an_edit_on_a_captured_claim_lands_stored_and_supplied_against_the_fake() {
    contract::an_edit_on_a_captured_claim_lands_stored_and_supplied(
        &InMemoryMemory::booted(),
        &fake_knowing_a_supplied_view(),
    )
    .await;
}

/// **A retraction on a claim lands alike, stored and supplied, against
/// the fake.**
#[tokio::test]
async fn a_retraction_on_a_captured_claim_lands_stored_and_supplied_against_the_fake() {
    contract::a_retraction_on_a_captured_claim_lands_stored_and_supplied(
        &InMemoryMemory::booted(),
        &fake_knowing_a_supplied_view(),
    )
    .await;
}

#[tokio::test]
async fn a_rename_of_a_supplied_handle_is_refused_against_the_fake() {
    contract::a_rename_of_a_supplied_handle_is_refused_not_a_silent_no_op(
        &fake_knowing_a_supplied_view(),
    )
    .await;
}

#[tokio::test]
async fn an_archive_of_a_supplied_handle_is_refused_against_the_fake() {
    contract::an_archive_of_a_supplied_handle_is_refused_not_a_silent_no_op(
        &fake_knowing_a_supplied_view(),
    )
    .await;
}

#[tokio::test]
async fn a_merge_naming_a_supplied_handle_is_refused_against_the_fake() {
    contract::a_merge_naming_a_supplied_handle_is_refused_not_a_silent_no_op(
        &fake_knowing_a_supplied_view(),
    )
    .await;
}

#[tokio::test]
async fn a_supplied_record_colliding_with_a_stored_row_is_refused_against_the_fake() {
    contract::a_supplied_record_colliding_with_a_stored_row_is_refused(&InMemoryMemory::booted())
        .await;
}

/// **On its own instance, not the shared `run_all` mount** — a fold's
/// renumbering is exactly the kind of state a case sharing a database
/// with dozens of others should not have to reason about.
#[tokio::test]
async fn a_stale_address_after_a_fold_says_where_it_went_against_the_fake() {
    contract::a_stale_address_after_a_fold_says_where_it_went(&InMemoryMemory::booted()).await;
}

#[test]
fn person_id_prefixes_a_bare_handle_but_respects_a_typed_one() {
    assert_eq!(EntityId::person("person:alpha").as_str(), "person:alpha");
    assert_eq!(EntityId::person("person:alpha").as_str(), "person:alpha");
}

#[test]
fn validate_subject_accepts_ids_and_rejects_adversarial_ones() {
    // **The set is this case's subject, not its setup.** What makes an
    // adversarial id adversarial is that its kind half names no kind the
    // set holds, so the refusals below are statements about the loaded set.
    crate::memory::kinds::load_shipped();
    assert!(validate_subject(&EntityId::person("person:alpha")).is_ok());
    assert!(validate_subject(&EntityId("project:jojobot-server".into())).is_ok());
    // Injection vectors: newline, pipe, header, fence, space, uppercase, empty.
    for bad in [
        "person:a|b",
        "a\nb",
        "### forged",
        "a`b",
        "a b",
        "Person:Alpha",
        "",
    ] {
        assert!(
            validate_subject(&EntityId(bad.into())).is_err(),
            "must reject {bad:?}"
        );
    }
}

/// Every shipped kind round-trips through its wire token, and nothing else
/// parses — a token nobody declared can never enter the store.
#[test]
fn the_shipped_kinds_round_trip_and_the_set_is_closed() {
    // **The set is this case's subject, not its setup.** The case walks the
    // shipped tokens and then asserts the set holds nothing else, so what
    // is loaded is the whole content of both assertions.
    crate::memory::kinds::load_shipped();
    let all = [
        (EntityKind::PERSON, "person"),
        (EntityKind::PROJECT, "project"),
        (EntityKind::PLACE, "place"),
        (EntityKind::EVENT, "event"),
        (EntityKind::WORK, "work"),
        (EntityKind::THING, "thing"),
        (EntityKind::ORG, "org"),
        (EntityKind::TOPIC, "topic"),
        (EntityKind::BOT, "bot"),
        (EntityKind::PET, "pet"),
        (EntityKind::RHYTHM, "rhythm"),
        (EntityKind::PROMISE, "promise"),
        (EntityKind::MACHINE, "machine"),
        (EntityKind::VIEW, "view"),
        (EntityKind::SESSION, "session"),
        (EntityKind::THREAD, "thread"),
        (EntityKind::ROLE, "role"),
    ];
    for (kind, token) in all {
        assert_eq!(kind.as_token(), token);
        assert_eq!(EntityKind::from_token(token), Some(kind));
    }
    assert_eq!(
        EntityKind::ALL.len(),
        all.len(),
        "every shipped kind is named above, and no other",
    );
    for unknown in ["receipt", "self", "Person", "", "peson"] {
        assert_eq!(
            EntityKind::from_token(unknown),
            None,
            "{unknown:?} is not a kind"
        );
    }
}

/// **A bot is an entity like any other.** Its handle validates by the same
/// grammar, so the codec, the guard, `search`, `recall` and `list_entities`
/// need no per-kind branch to carry it.
#[test]
fn a_bot_handle_is_an_ordinary_entity_id() {
    // **The set is this case's subject, not its setup.** The claim is that
    // `bot` sits in the set beside the other kinds and is read the same
    // way, so what is loaded is the thing being asserted.
    crate::memory::kinds::load_shipped();
    let id = EntityId("bot:otto".into());
    assert_eq!(id.as_str(), "bot:otto");
    assert_eq!(id.kind(), Some(EntityKind::BOT));
    assert!(validate_subject(&id).is_ok());
    // And it is spelled out on a bare handle, exactly as every non-person is.
    assert_eq!(EntityId::person("bot:otto").as_str(), "bot:otto");
}

/// An id is `kind:slug` — the kind and the slug are readable off it, which is
/// what lets the guard compare slugs and the codec stamp a kind.
#[test]
fn an_id_splits_into_its_kind_and_slug() {
    // **The set is this case's subject, not its setup.** The kind half of
    // the split is answered from the set, so what comes back is a read of
    // it rather than of the string.
    crate::memory::kinds::load_shipped();
    let id = EntityId("project:jojobot-server".into());
    assert_eq!(id.as_str(), "project:jojobot-server");
    assert_eq!(id.kind(), Some(EntityKind::PROJECT));
    assert_eq!(id.slug(), "jojobot-server");
    // A malformed id yields no kind rather than panicking — reads never hard-fail.
    assert_eq!(EntityId("nonsense".into()).kind(), None);
}

/// The grammar is `kind:slug` with slug `[a-z0-9-]+`: an unknown kind, a
/// missing kind, an underscore, or a second colon is not an entity id.
/// **A session is readable and is not writable through the memory verbs.**
///
/// 🚨 **Selectable must not mean writable.** A session is given a handle so
/// a bot can ask the graph about its own past runs. A memory write onto
/// that handle would step past the state machine the session verbs hold —
/// bound to its bot, wrapped once and never reopened, only an abandoned run
/// walking back — none of which the memory path knows about.
///
/// **Both halves, and they are the point:** the write gate refuses it and
/// the read gate does NOT. A check that refused both would make the handle
/// useless, and the handle is the whole capability.
///
/// The ordinary kind is here because a gate that refused every subject
/// would satisfy the first assertion on its own.
#[test]
fn a_session_is_refused_by_the_write_gate_and_allowed_by_the_read_gate() {
    // A handle is parsed against the kinds this process loaded, so the
    // set arrives the way a boot delivers it.
    let _booted = testing::InMemoryMemory::booted();
    let run = EntityId("session:contract-gamma-run".into());
    let ordinary = EntityId::person("person:milhouse");

    assert!(
        validate_write_subject(&run).is_err(),
        "a memory write onto a session steps past the verbs that hold its life",
    );
    assert!(
        validate_subject(&run).is_ok(),
        "…and reading one is exactly what giving it a handle was for",
    );
    assert!(
        validate_write_subject(&ordinary).is_ok(),
        "…and an ordinary subject is still written, or the gate refuses everything",
    );
}

#[test]
fn validate_subject_enforces_the_kind_slug_grammar() {
    // **The set is this case's subject, not its setup.** The grammar's
    // first half is "a kind", and only the set can say whether a token is
    // one.
    crate::memory::kinds::load_shipped();
    for good in [
        "person:alpha",
        "topic:widgets",
        "org:north-trail-club",
        "thing:red-bike",
    ] {
        assert!(
            validate_subject(&EntityId(good.into())).is_ok(),
            "must accept {good:?}"
        );
    }
    for bad in [
        "alpha",           // no kind
        "receipt:il-2026", // not one of the nine
        "person:",         // empty slug
        ":alpha",          // empty kind
        "person:a_b",      // underscore is out of the slug charset
        "person:a:b",      // one colon only
    ] {
        assert!(
            validate_subject(&EntityId(bad.into())).is_err(),
            "must reject {bad:?}"
        );
    }
}

/// The compound address `doc#local-id` — what `recall` hands back and
/// `update_fact` targets — round-trips, and a malformed one is rejected.
#[test]
fn a_fact_address_round_trips_through_its_wire_form() {
    // **This case runs in a booted process.** It parses a handle and asserts
    // about something else, so the set is setup — and setup comes from
    // standing a store up, filled from what that store holds.
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let addr = FactAddress::new(EntityId::person("person:alpha"), FactId("f3".into()));
    assert_eq!(addr.to_string(), "person:alpha#f3");
    assert_eq!(FactAddress::parse("person:alpha#f3").unwrap(), addr);
    for bad in [
        "person:alpha",
        "#f3",
        "person:alpha#",
        "person:alpha#f 3",
        "nope:x#f1",
        "",
    ] {
        assert!(FactAddress::parse(bad).is_err(), "must reject {bad:?}");
    }
}

/// Both lifecycle states have tokens; an unknown or blank cell degrades to
/// active (the tolerant-read rule: never drop a fact over a bad cell).
///
/// And every **legacy token reads as archived**. `superseded` and
/// `retracted` were once two variants and are now one; `negated` is
/// older still, from before negation was a status at all. Rows written
/// under any of the three are on disk, and a schema removal must never
/// hard-fail a read any more than a schema addition may. Archived is the
/// honest landing spot for all three: the behaviour that mattered,
/// excluded-by-default, is identical.
#[test]
fn fact_status_tokens_round_trip_and_every_legacy_token_reads_as_archived() {
    for status in [FactStatus::Active, FactStatus::Archived] {
        assert_eq!(FactStatus::from_token(status.as_token()), status);
    }
    for legacy in ["superseded", "retracted", "negated"] {
        assert_eq!(
            FactStatus::from_token(legacy),
            FactStatus::Archived,
            "a row from before the two statuses merged still reads, and stays out of a \
                 default search: {legacy}"
        );
    }
    assert_eq!(FactStatus::from_token(""), FactStatus::Active);
    assert_eq!(FactStatus::from_token("garbled"), FactStatus::Active);
}

/// Four shapes, no more — and each has two spellings on purpose: the token a
/// caller passes (and the table stores) and the schema.org name a response
/// renders. `membership`/`memberOf` and `attendance`/`attendee` are where
/// they diverge; input stays lowercase, always.
#[test]
fn the_edge_shapes_round_trip_and_the_set_is_closed() {
    let all = [
        (EdgeShape::Location, "location", "location"),
        (EdgeShape::Membership, "membership", "memberOf"),
        (EdgeShape::Attendance, "attendance", "attendee"),
        (EdgeShape::About, "about", "about"),
        (EdgeShape::Connection, "connection", "relatedTo"),
    ];
    for (shape, token, name) in all {
        assert_eq!(shape.as_token(), token);
        assert_eq!(shape.as_name(), name);
        assert_eq!(EdgeShape::from_token(token), Some(shape));
    }
    assert_eq!(
        EdgeShape::ALL.len(),
        5,
        "four shapes from M2, plus the untyped one events point with"
    );
    // A response name is NOT an input token: the input grammar is unchanged.
    for unknown in ["memberOf", "attendee", "knows", "Location", "", "locaton"] {
        assert_eq!(
            EdgeShape::from_token(unknown),
            None,
            "{unknown:?} is not a shape token"
        );
    }
}

/// **The fifth shape, and the whole point of it is that it is not the
/// fourth.**
///
/// An event may point at entities whose relationship nobody recorded. The
/// tempting move is to file those as `about`, since `about` already takes
/// any kind — and that is exactly the move this shape exists to refuse.
/// `about` ASSERTS that the record is about that entity, which is a claim
/// somebody made; a `connection` ADMITS a link whose meaning is unknown.
/// Collapsing the two launders an unknown into an assertion, and every
/// reader downstream then reads a claim nobody ever made.
///
/// The pointer is real either way — that is the other half. Only the NATURE
/// of the link is deferred, so it must still be walkable, which is what
/// `an_untyped_edge_is_walkable_like_any_other` holds it to.
#[test]
fn the_untyped_shape_is_its_own_shape_and_never_about() {
    // **This case runs in a booted process.** It parses a handle and asserts
    // about something else, so the set is setup — and setup comes from
    // standing a store up, filled from what that store holds.
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    assert_eq!(EdgeShape::Connection.as_token(), "connection");
    assert_eq!(
        EdgeShape::from_token("connection"),
        Some(EdgeShape::Connection)
    );
    assert_ne!(
        EdgeShape::Connection,
        EdgeShape::About,
        "an admitted link and an asserted one are not the same edge"
    );
    // …and they do not share a response name either, or a reader sorting by
    // name would put them back together.
    assert_ne!(EdgeShape::Connection.as_name(), EdgeShape::About.as_name());

    // **Any kind, like `about`** — an event points at whatever it points at,
    // and refusing a kind here would be jojobot deciding what the link
    // means, which is the thing it does not know.
    assert_eq!(EdgeShape::Connection.object_kind(), None);
    for object in ["person:alpha", "place:north-trail", "event:winter-fest"] {
        assert!(
            validate_edge(&Edge::new(EdgeShape::Connection, EntityId(object.into()))).is_ok(),
            "an untyped edge takes {object}"
        );
    }
    // The id grammar is still enforced: unknown MEANING is not unknown SHAPE.
    assert!(
        validate_edge(&Edge::new(EdgeShape::Connection, EntityId("a|b".into()))).is_err(),
        "a malformed handle is malformed whatever the link means"
    );
}

/// Each shape pins its object's kind — `about` is the one open shape. A
/// `location` pointing at a person is a mis-drawn edge, not a nuance, and it
/// is refused before anything is written.
#[test]
fn an_edge_object_must_be_the_kind_its_shape_requires() {
    // **The set is this case's subject, not its setup.** The kind of the
    // object is what this refuses on, read from the set.
    crate::memory::kinds::load_shipped();
    let ok = [
        (EdgeShape::Location, "place:north-trail"),
        (EdgeShape::Membership, "org:north-trail-club"),
        (EdgeShape::Attendance, "event:winter-fest"),
        (EdgeShape::About, "topic:widgets"),
        (EdgeShape::About, "person:alpha"),
    ];
    for (shape, object) in ok {
        assert!(
            validate_edge(&Edge::new(shape, EntityId(object.into()))).is_ok(),
            "{shape} must accept {object}"
        );
    }
    let bad = [
        (EdgeShape::Location, "person:alpha"),
        (EdgeShape::Membership, "place:north-trail"),
        (EdgeShape::Attendance, "project:atlas"),
    ];
    for (shape, object) in bad {
        let err = validate_edge(&Edge::new(shape, EntityId(object.into())))
            .expect_err("a wrong-kind object must be refused");
        assert!(
            matches!(err, MemoryError::InvalidEdge(_)),
            "expected InvalidEdge for {shape}/{object}, got {err:?}"
        );
    }
    // The object is an entity id first: the grammar is checked as a subject's is.
    let err = validate_edge(&Edge::new(EdgeShape::About, EntityId("a|b".into())))
        .expect_err("a malformed object must be refused");
    assert!(matches!(err, MemoryError::InvalidSubject(_)), "got {err:?}");
}

/// A **bare `\r`** is refused exactly as `\n` is in content: a claim is the
/// headline, and search, the boot and the receipts rely on it being one line.
#[test]
fn a_bare_carriage_return_is_refused_like_a_newline_in_content() {
    for bad in ["hello\rworld", "trailing\r", "\rleading", "a\r\nb", "a\nb"] {
        assert!(
            validate_content(bad).is_err(),
            "content must refuse a line break: {bad:?}"
        );
    }
    assert!(validate_content("hello world").is_ok());
}

/// **The refusal says where the rest of the text goes.** A claim is one line
/// and `details` holds paragraph breaks, so a caller refused for a line break
/// is told the argument by name.
#[test]
fn a_multi_line_content_refusal_names_details() {
    let said = validate_content("first line\nsecond line")
        .expect_err("a line break is refused")
        .to_string();
    assert!(said.contains("details"), "{said}");
}

/// **An entity answers to more than one name.** The display name is what it
/// is *called*; an alias is what someone actually says — the nickname, the
/// short form, the initials. Without them the guard cannot recognize a name
/// the user uses every day, and search cannot find it.
///
/// An alias is a plain one-line label, exactly as `name` is, with one extra
/// rule: **no comma**, because the frontmatter carries the set on one
/// comma-separated line and an alias with a comma in it would silently
/// become two.
#[test]
fn an_alias_is_a_plain_label_and_never_carries_the_separator() {
    assert!(validate_aliases(&["Cosme Fulanito".into(), "H.".into()]).is_ok());
    assert!(
        validate_aliases(&[]).is_ok(),
        "no aliases is the ordinary case"
    );
    for bad in ["", "   ", "one, two", "two\nlines", "back`tick"] {
        assert!(
            validate_aliases(&[bad.into()]).is_err(),
            "must refuse the alias {bad:?}"
        );
    }
}

/// Aliases patch like every other metadata field: `None` leaves them alone,
/// `Some` replaces the whole set — including `Some(vec![])`, which is how a
/// caller says "it has none", a thing they must be able to say.
#[test]
fn an_alias_set_is_replaced_whole_or_left_alone() {
    let mut entity = Entity {
        id: EntityId::person("person:alpha"),
        kind: EntityKind::PERSON,
        name: "Alpha".into(),
        aliases: vec!["Al".into()],
        source: "user-named".into(),
        crm: None,
        parent: None,
        boot: Boot::OnDemand,
        merged_into: None,
        badge: None,
        archived: None,
    };

    apply_entity_patch(
        &mut entity,
        &EntityPatch {
            source: Some("crm-card".into()),
            ..Default::default()
        },
    )
    .expect("patch ok");
    assert_eq!(
        entity.aliases,
        vec!["Al".to_string()],
        "an omitted field is left alone"
    );

    apply_entity_patch(
        &mut entity,
        &EntityPatch {
            aliases: Some(vec!["  Al  ".into(), "Alph".into()]),
            ..Default::default()
        },
    )
    .expect("patch ok");
    assert_eq!(
        entity.aliases,
        vec!["Al".to_string(), "Alph".to_string()],
        "the set is replaced whole, and trimmed the way a name is"
    );

    apply_entity_patch(
        &mut entity,
        &EntityPatch {
            aliases: Some(Vec::new()),
            ..Default::default()
        },
    )
    .expect("patch ok");
    assert!(
        entity.aliases.is_empty(),
        "an empty set is a set, not an omission"
    );

    assert!(
        apply_entity_patch(
            &mut entity,
            &EntityPatch {
                aliases: Some(vec!["one, two".into()]),
                ..Default::default()
            }
        )
        .is_err(),
        "a malformed alias is refused before anything is mutated"
    );
}

/// The **labels** of an entity: its name and every alias, which is the set
/// the guard screens and search indexes. One definition, so "what is this
/// thing called" cannot come to mean two different things in two places.
#[test]
fn an_entitys_labels_are_its_name_and_its_aliases() {
    let entity = |name: &str, aliases: Vec<String>| Entity {
        id: EntityId::person("person:alpha"),
        kind: EntityKind::PERSON,
        name: name.into(),
        aliases,
        source: "user-named".into(),
        crm: None,
        parent: None,
        boot: Boot::OnDemand,
        merged_into: None,
        badge: None,
        archived: None,
    };
    assert_eq!(
        entity("Alpha", vec!["Al".into(), "Alph".into()]).labels(),
        vec!["Alpha", "Al", "Alph"],
        "the display name leads; it is the one the entity is filed under"
    );
    assert_eq!(entity("Alpha", Vec::new()).labels(), vec!["Alpha"]);
    assert!(
        entity("", vec!["  ".into()]).labels().is_empty(),
        "an entity with nothing written on it has no labels, not blank ones"
    );
}
#[test]
fn provenance_tokens_round_trip_and_degrade_to_inference() {
    assert_eq!(Provenance::from_token("testimony"), Provenance::Testimony);
    assert_eq!(Provenance::from_token("inference"), Provenance::Inference);
    assert_eq!(Provenance::from_token(""), Provenance::Inference);
    assert_eq!(Provenance::from_token("garbled"), Provenance::Inference);
}

/// **A bot's seats are the number it carries, and anything that is not a whole
/// number above zero reads as no key at all.** Each non-number is paired with
/// the number that is read, or a reader that ignored the key would pass them.
#[test]
fn a_bots_seats_read_a_whole_number_and_nothing_else() {
    let held = |value: &str| -> BTreeMap<String, String> {
        [(RULE_SEATS.to_string(), value.to_string())]
            .into_iter()
            .collect()
    };
    assert_eq!(rule_seats_of(&held("8")), 8);
    assert_eq!(rule_seats_of(&held(" 12 ")), 12);
    let default = crate::text::CARRIED_RULES;
    assert_eq!(rule_seats_of(&BTreeMap::new()), default);
    assert_eq!(rule_seats_of(&held("0")), default);
    assert_eq!(rule_seats_of(&held("many")), default);
    assert_eq!(rule_seats_of(&held("-3")), default);
}

/// **The seats-full sentence and the displaced rule follow the bot's own
/// seats.** At the count nothing is displaced; one over it, the oldest is.
#[test]
fn the_seat_status_counts_against_the_bots_own_seats() {
    let starred = |n: usize| -> Vec<Fact> {
        (1..=n)
            .map(|i| Fact {
                id: FactId(format!("f{i}")),
                home: EntityId("bot:gamma".into()),
                subject: EntityId("bot:gamma".into()),
                content: format!("rule {i}"),
                details: None,
                provenance: Provenance::Testimony,
                standing: Standing::Settled,
                status: FactStatus::Active,
                recorded_at: jiff::civil::date(2026, 8, 1),
                happened_at: None,
                happened_through: None,
                edge: None,
                fields: [("starred".to_string(), "true".to_string())]
                    .into_iter()
                    .collect(),
                refs: Vec::new(),
                derived_from: None,
                stands_for: Vec::new(),
                inserted_at: None,
                stale_after: None,
            })
            .collect()
    };
    assert!(carried_seats_status(&starred(7), 8).is_none());
    let full = carried_seats_status(&starred(8), 8).expect("every seat taken says so");
    assert_eq!(full.starred, 8);
    assert!(full.dropped.is_none(), "nothing is displaced at the count");
    let over = carried_seats_status(&starred(9), 8).expect("one over says so");
    assert_eq!(
        over.dropped.expect("the oldest is displaced").to_string(),
        "bot:gamma#f1"
    );
}

/// **A fold carries the duplicate's display name and then its aliases, and
/// leaves out a name the survivor already answers to, whatever its case, and a
/// name repeated within the carried list.**
#[test]
fn a_fold_carries_each_new_name_once() {
    let carried = names_to_carry(
        "Kept",
        &["Shared".to_string()],
        "Spare",
        &[
            "SHARED".to_string(),
            "Extra".to_string(),
            "extra".to_string(),
        ],
    );
    assert_eq!(
        carried,
        vec!["Spare".to_string(), "Extra".to_string()],
        "the display name comes first, a name the survivor wears is skipped, and a repeat is kept once",
    );
}

/// **A ceiling is a whole number of zero or more, and anything else is refused
/// where it is written** — by capture and by edit alike, because both run the
/// same validator. Each refusal is paired with a value that reads.
#[test]
fn a_ceiling_that_does_not_read_as_a_whole_number_is_refused() {
    let one = |key: &str, value: &str| -> BTreeMap<String, String> {
        [(key.to_string(), value.to_string())].into_iter().collect()
    };
    for key in [THOUGHT_CAPACITY, THOUGHT_BODY_CAP] {
        for fine in ["0", "5", " 12 "] {
            assert!(
                validate_fields(&one(key, fine)).is_ok(),
                "{key} = {fine:?} reads as a whole number"
            );
        }
        for bad in ["unlimited", "5.0", "-1", "", "5 chars"] {
            assert!(
                matches!(
                    validate_fields(&one(key, bad)),
                    Err(MemoryError::InvalidFact(_))
                ),
                "{key} = {bad:?} must be refused"
            );
        }
        // The key is matched as the store keeps it, trimmed.
        assert!(
            validate_fields(&one(&format!(" {key}"), "unlimited")).is_err(),
            "a padded key is the same key"
        );
    }
    // Another key holding the same words is none of this validator's business.
    assert!(validate_fields(&one("note", "unlimited")).is_ok());
}

/// **The mark a stored link wears is jojobot's, and a caller cannot write it.**
///
/// A value whose item starts with the mark is read back as the id of whatever
/// wears that badge, so a caller who could write one could draw a link to any
/// thing without the guard that checks it exists. Every item of a list is held
/// to it, and each refusal is paired with a value that reads.
#[test]
fn a_caller_cannot_write_the_stored_mark_into_a_field_value() {
    let one = |value: &str| -> BTreeMap<String, String> {
        [("blocks".to_string(), value.to_string())]
            .into_iter()
            .collect()
    };
    for bad in ["@#k7h2mn", "  @#k7h2mn", "work:sigma, @#k7h2mn"] {
        let refused =
            validate_fields(&one(bad)).expect_err("a value carrying the stored mark is refused");
        assert!(
            matches!(refused, MemoryError::InvalidFact(_)),
            "{bad:?} is refused as an invalid fact: {refused:?}"
        );
    }
    for fine in [
        "work:sigma",
        "mail me @ home",
        "see @#",
        "email a@#b.example",
    ] {
        assert!(
            validate_fields(&one(fine)).is_ok(),
            "{fine:?} does not start an item with the stored mark"
        );
    }
}
