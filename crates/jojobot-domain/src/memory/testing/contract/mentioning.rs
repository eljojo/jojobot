use super::support::{add, capture, ensure};
use super::*;

/// **A store this suite can rename a handle in.**
///
/// There is no rename verb and this slice does not add one, so the only way
/// to move a handle is the way it really happens: an edit outside jojobot.
/// Each store stages it in its own words, and the case that reads the
/// result is written once.
#[async_trait::async_trait]
pub trait Rehandles: Send + Sync {
    /// Move a row from one handle to another, keeping the badge it wears.
    async fn rehandle(&self, from: &EntityId, to: &EntityId);
    /// Stage the rename event a real verb would leave behind, so a lookup
    /// on the old handle can be proved to fall through to the new one.
    async fn note_former_handle(&self, event: FormerHandle);
}

/// 🚨 **A mention is stored as the badge and read back as the handle.**
///
/// A handle is a path: the kind and the slug, both of which move when a
/// thing is renamed or retyped. Text holding the spelling is text pointing
/// at nothing the day either moves, and nothing indexes it or knows to go
/// and repair it.
///
/// **Both halves in one read**, because either alone passes on a build that
/// does nothing: the stored form must NOT carry the handle, and the read
/// form must.
///
/// ⭐ **Four mentions across four kinds in one claim**, which is the shape
/// the requirement was stated in — a case with one mention proves a
/// mechanism that stops at the first.
pub async fn a_mention_is_stored_as_a_badge_and_read_back_as_a_handle<
    M: Memory + ?Sized,
    B: Memory + ?Sized,
>(
    mentioning: &M,
    bare: &B,
) {
    let author = EntityId::person("person:contract-mention-author");
    let named = [
        ("person:contract-mention-rider", "The Rider"),
        ("thing:contract-mention-cart", "The Cart"),
        ("place:contract-mention-yard", "The Yard"),
        ("pet:contract-mention-dog", "The Dog"),
    ];
    ensure(mentioning, &author).await;
    for (handle, name) in named {
        mentioning
            .add_entity(NewEntity::new(EntityId(handle.into()), name, "the roster"))
            .await
            .expect("the fixture is written")
            .written()
            .expect("nothing on this store collides with it");
    }

    let content = concat!(
        "@person:contract-mention-rider took @pet:contract-mention-dog to ",
        "@place:contract-mention-yard on @thing:contract-mention-cart",
    );
    let written = capture(
        mentioning,
        NewFact::about(author.clone(), content, date(2026, 4, 18)),
    )
    .await;

    // ① **What the store keeps carries no handle.** Read underneath the
    // layer that renders, because every read above it would show the
    // handles whether or not they were stored.
    let stored = bare
        .recall(&author)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == written.id)
        .expect("the claim is there")
        .content;
    for (handle, _) in named {
        assert!(
            !stored.contains(handle),
            "the stored claim still carries the handle {handle}, so a rename breaks it: \
             {stored}",
        );
    }
    assert_eq!(
        stored.matches(mention::MARK).count(),
        named.len(),
        "every mention was stored resolved, not just the first: {stored}",
    );

    // ⭐ **A correction is where a mention most often arrives**, because
    // that is where somebody rewrites the sentence — so an edit resolves
    // what it writes exactly as a capture does.
    mentioning
        .update_fact(
            &written.address(),
            FactPatch {
                content: Some(
                    "@person:contract-mention-rider walked @pet:contract-mention-dog home".into(),
                ),
                // **The nuance is text too**, and it leaves the store by a
                // line of its own — so it is asserted rather than assumed
                // to follow the claim above it.
                details: Some("along @place:contract-mention-yard".into()),
                ..Default::default()
            },
        )
        .await
        .expect("the correction lands")
        .written()
        .expect("nothing blocks it");
    let corrected = bare
        .recall(&author)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == written.id)
        .expect("the claim is there");
    let under = corrected.details.clone().expect("the nuance is stored");
    let corrected = corrected.content;
    assert!(
        !corrected.contains("person:contract-mention-rider")
            && corrected.matches(mention::MARK).count() == 2,
        "the correction stored the handles it was written with, so an edit undoes what \
         the capture bought: {corrected}",
    );
    assert!(
        !under.contains("place:contract-mention-yard") && under.matches(mention::MARK).count() == 1,
        "the correction's nuance stored the handle it was written with: {under}",
    );

    // ② …and the read gives every one of them back as a handle.
    let read = mentioning
        .recall(&author)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == written.id)
        .expect("the claim is there")
        .content;
    for handle in ["person:contract-mention-rider", "pet:contract-mention-dog"] {
        assert!(
            read.contains(handle),
            "the read left out the mention of {handle}: {read}",
        );
    }
}

/// 🚨 **A mention of a thing that moves renders as where it is now**, with
/// nothing rewritten anywhere.
///
/// **Paired with the stored form in the same read**, because a build that
/// rewrote every claim on a rename would pass the first assertion and fail
/// the second — and the two are the difference between a pointer and a
/// find-and-replace.
pub async fn a_mention_follows_a_thing_that_is_rehandled<M: Memory + ?Sized, B: Memory + ?Sized>(
    mentioning: &M,
    bare: &B,
    rehandles: &dyn Rehandles,
) {
    let author = EntityId::person("person:contract-mention-moved");
    let was = EntityId("thing:contract-mention-was".into());
    let now = EntityId("work:contract-mention-now".into());
    ensure(mentioning, &author).await;
    mentioning
        .add_entity(NewEntity::new(was.clone(), "The Moved One", "the roster"))
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");

    let written = capture(
        mentioning,
        NewFact::about(
            author.clone(),
            "the survey went out on @thing:contract-mention-was",
            date(2026, 4, 18),
        ),
    )
    .await;
    let before = read_claim(mentioning, &author, &written.id).await;
    assert!(
        before.contains(was.as_str()),
        "the mention reads as the handle it was written with: {before}",
    );

    // **A retype is a rename** (rule 202): the kind moves and the row is
    // the same row, which is exactly what the badge is for.
    rehandles.rehandle(&was, &now).await;

    let after = read_claim(mentioning, &author, &written.id).await;
    assert!(
        after.contains(now.as_str()),
        "the mention did not follow the thing to its new handle: {after}",
    );
    assert!(
        !after.contains(was.as_str()),
        "the mention still reads as the handle nobody answers to: {after}",
    );
    // ⭐ **And nothing was rewritten to do it.** The claim in the store says
    // exactly what it said before the move.
    let stored = bare
        .recall(&author)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == written.id)
        .expect("the claim is there")
        .content;
    assert!(
        !stored.contains(now.as_str()) && !stored.contains(was.as_str()),
        "the stored claim was rewritten, so this is find-and-replace rather than a \
         pointer: {stored}",
    );
}

/// 🚨 **A stale handle resolves through its own rename history, and a
/// handle nothing ever answered to still misses — over a real store, not
/// only the pure function.**
///
/// No verb writes a rename event yet, so `rehandles` stages both halves of
/// the state one would leave: the row moved, and the event recorded. Both
/// assertions in one read, because a resolver answering every handle would
/// pass the first alone.
pub async fn a_stale_handle_resolves_through_its_rename_history<M: Memory + ?Sized>(
    store: &M,
    rehandles: &dyn Rehandles,
) {
    let was = EntityId("thing:contract-former-handle-was".into());
    let now = EntityId("work:contract-former-handle-now".into());
    store
        .add_entity(NewEntity::new(was.clone(), "The Renamed One", "the roster"))
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");
    let badge = store
        .list_entities(None)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|e| e.id == was)
        .expect("the fixture is there")
        .badge
        .expect("a written row wears a badge");

    rehandles.rehandle(&was, &now).await;
    rehandles
        .note_former_handle(FormerHandle {
            former: was.clone(),
            badge,
            changed_at: date(2026, 4, 18),
        })
        .await;

    let known = store.list_entities(None).await.expect("the store answers");
    let former = store.former_handles().await.expect("the store answers");

    let resolved = resolve_handle(&was, &known, &former);
    assert_eq!(
        resolved.map(|e| &e.id),
        Some(&now),
        "the old handle did not resolve to the thing's current one: {resolved:?}",
    );

    let never = EntityId("person:contract-former-handle-never".into());
    assert_eq!(
        resolve_handle(&never, &known, &former),
        None,
        "a handle nothing ever answered to must stay a miss",
    );
}

/// 🚨 **A child's parent pointer follows a rename, resolved on the way
/// out — the same shape an edge and a ref already get, not the shape a
/// claim's home or an alias got.**
///
/// **`parent` is never a lookup key.** `children` reads every entity and
/// filters in memory rather than querying by the column, so nothing here
/// needs the storage stability a badge buys — only what a reader sees
/// needs to change, which is what resolving on read is for.
///
/// **`children` itself is the proof that matters**, not just the field:
/// a fix that only patched `Entity::parent` and left the reverse lookup
/// reading the raw stored value would still lose a session asking "what
/// is under this thing now."
pub async fn a_childs_parent_pointer_follows_a_rename<M: Memory + ?Sized>(
    store: &M,
    rehandles: &dyn Rehandles,
) {
    let was = EntityId("person:contract-parent-was".into());
    let now = EntityId("work:contract-parent-now".into());
    store
        .add_entity(NewEntity::new(
            was.clone(),
            "Contract Parent Was",
            "the roster",
        ))
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");
    let badge = store
        .list_entities(None)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|e| e.id == was)
        .expect("the fixture is there")
        .badge
        .expect("a written row wears a badge");

    let child = EntityId("person:contract-parent-child".into());
    store
        .add_entity(NewEntity {
            parent: Some(was.clone()),
            ..NewEntity::new(child.clone(), "Contract Parent Child", "the roster")
        })
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");

    rehandles.rehandle(&was, &now).await;
    rehandles
        .note_former_handle(FormerHandle {
            former: was.clone(),
            badge,
            changed_at: date(2026, 4, 19),
        })
        .await;

    let held = store
        .list_entities(None)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|e| e.id == child)
        .expect("the child is there");
    assert_eq!(
        held.parent.as_ref(),
        Some(&now),
        "the child's parent did not follow the rename: {held:?}",
    );

    let kids = store
        .children(&now)
        .await
        .expect("children reads under the current handle");
    assert_eq!(
        kids,
        vec![child],
        "the reverse lookup did not find the child through the renamed parent: {kids:?}",
    );
}

/// 🚨 **An edge follows a rename, and nothing rewrites the claim to do
/// it** (rule 268). An edge's object is stored as the badge the entity
/// wears rather than the handle it was drawn with, so the row never goes
/// stale — the same handle-to-badge resolution `bare` already gives a
/// capture's subject.
///
/// **Paired against `bare`, with no `mentioning` involved**, because the
/// resolution now lives in the store itself: a build that only fixed the
/// decorator would pass `mentioning` and fail `bare`.
pub async fn an_edge_follows_a_thing_that_is_rehandled<M: Memory + ?Sized, B: Memory + ?Sized>(
    mentioning: &M,
    bare: &B,
    rehandles: &dyn Rehandles,
) {
    let subject = EntityId::person("person:contract-edge-subject");
    let was = EntityId("thing:contract-edge-was".into());
    let now = EntityId("work:contract-edge-now".into());
    ensure(mentioning, &subject).await;
    mentioning
        .add_entity(NewEntity::new(
            was.clone(),
            "The Moved Object",
            "the roster",
        ))
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");
    let badge = mentioning
        .list_entities(None)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|e| e.id == was)
        .expect("the fixture is there")
        .badge
        .expect("a written row wears a badge");

    let mut fact = NewFact::about(
        subject.clone(),
        "drew an edge at the moved object",
        date(2026, 4, 20),
    );
    fact.edge = Some(Edge::new(EdgeShape::About, was.clone()));
    let written = capture(mentioning, fact).await;
    let before = mentioning
        .recall(&subject)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == written.id)
        .expect("the claim is there");
    assert_eq!(
        before.edge.map(|e| e.object),
        Some(was.clone()),
        "the edge reads as the handle it was drawn with",
    );

    rehandles.rehandle(&was, &now).await;
    rehandles
        .note_former_handle(FormerHandle {
            former: was.clone(),
            badge,
            changed_at: date(2026, 4, 20),
        })
        .await;

    let after = mentioning
        .recall(&subject)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == written.id)
        .expect("the claim is there");
    assert_eq!(
        after.edge.as_ref().map(|e| &e.object),
        Some(&now),
        "the edge did not follow the thing to its new handle: {after:?}",
    );

    // The bare store follows the rename too — the fix is in storage, not
    // only in the layer that renders for a reader.
    let stored = bare
        .recall(&subject)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == written.id)
        .expect("the claim is there");
    assert_eq!(
        stored.edge.as_ref().map(|e| &e.object),
        Some(&now),
        "the bare store did not follow the rename, so the fix is still only in the decorator: \
         {stored:?}",
    );
}

/// 🚨 **A ref follows a rehandled thing exactly as an edge does**, for
/// the same reason: it is stored as the badge, not the handle (rule 268).
///
/// **Paired against `bare`, for the reason the edge case is**: the fix
/// lives in the store, not the decorator.
pub async fn a_ref_follows_a_thing_that_is_rehandled<M: Memory + ?Sized, B: Memory + ?Sized>(
    mentioning: &M,
    bare: &B,
    rehandles: &dyn Rehandles,
) {
    let subject = EntityId::person("person:contract-ref-subject");
    let was = EntityId("thing:contract-ref-was".into());
    let now = EntityId("work:contract-ref-now".into());
    ensure(mentioning, &subject).await;
    mentioning
        .add_entity(NewEntity::new(
            was.clone(),
            "The Referenced Object",
            "the roster",
        ))
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");
    let badge = mentioning
        .list_entities(None)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|e| e.id == was)
        .expect("the fixture is there")
        .badge
        .expect("a written row wears a badge");

    let mut fact = NewFact::about(
        subject.clone(),
        "pointed at the referenced object",
        date(2026, 4, 21),
    );
    fact.refs = vec![was.clone()];
    let written = capture(mentioning, fact).await;
    let before = mentioning
        .recall(&subject)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == written.id)
        .expect("the claim is there");
    assert_eq!(
        before.refs,
        vec![was.clone()],
        "the ref reads as the handle it was written with",
    );

    rehandles.rehandle(&was, &now).await;
    rehandles
        .note_former_handle(FormerHandle {
            former: was.clone(),
            badge,
            changed_at: date(2026, 4, 21),
        })
        .await;

    let after = mentioning
        .recall(&subject)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == written.id)
        .expect("the claim is there");
    assert_eq!(
        after.refs,
        vec![now.clone()],
        "the ref did not follow the thing to its new handle: {after:?}",
    );

    // The bare store follows the rename too.
    let stored = bare
        .recall(&subject)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == written.id)
        .expect("the claim is there");
    assert_eq!(
        stored.refs,
        vec![now],
        "the bare store did not follow the rename, so the fix is still only in the decorator: \
         {stored:?}",
    );
}

/// 🚨 **A reference-typed field value resolves in THING scope exactly as it
/// does in RECORD scope, after a rename.**
///
/// `Memory::fields` folds a thing's writes into the map a thing-scope filter
/// compares against — the default scope, so this is the shape most callers
/// write. `render_fact` walks a reference-typed field value through
/// `resolve_handle` on every fact-returning read, which is what a
/// record-scope filter compares against instead. Before this, `fields` was a
/// bare delegate with no resolution, so the two disagreed about the same key
/// on the same thing: a filter for today's handle missed, in thing scope
/// alone, an object that points there right now.
///
/// **Paired in one read**: found under today's handle, and NOT found under
/// the handle the query never sent — the trap is that "not found under the
/// stale handle" passes identically on a build that finds nothing at all, so
/// the positive half has to stand beside it.
pub async fn a_reference_typed_field_value_resolves_in_thing_scope_after_a_rename<
    M: Memory + ?Sized,
>(
    mentioning: &M,
) {
    mentioning
        .declare_type(DeclaredType::new(
            "contract-scope-pet",
            vec![Field::new("owner", ValueType::Reference)],
        ))
        .await
        .expect("the type is declared");

    let was = EntityId::person("person:contract-scope-owner-was");
    mentioning
        .add_entity(NewEntity::new(was.clone(), "The Old Owner", "the roster"))
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");

    let pet = EntityId("pet:contract-scope-pet".into());
    mentioning
        .add_entity(NewEntity::new(pet.clone(), "The Pet", "the roster"))
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");

    capture(
        mentioning,
        NewFact {
            fields: [("owner".to_string(), was.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(pet.clone(), "belongs to its owner", date(2026, 7, 1))
        },
    )
    .await;

    let now = EntityId::person("person:contract-scope-owner-now");
    mentioning
        .rename_entity(&was, &now, None, date(2026, 7, 2), None)
        .await
        .expect("the rename lands")
        .written()
        .expect("nothing collides with the destination");

    let query = |value: &str| graph::GraphQuery {
        select: graph::Selection {
            fields: vec![graph::FieldFilter::holding("owner", value)],
            ..graph::Selection::default()
        },
        include: graph::Include {
            facts: false,
            prose: false,
            stood_for: false,
        },
        follow: None,
        history: None,
    };

    let found_by_new = graph::walk(mentioning, &query(now.as_str()))
        .await
        .expect("a thing-scope filter on a reference key must resolve");
    assert_eq!(
        found_by_new
            .objects
            .iter()
            .map(|o| o.entity.id.clone())
            .collect::<Vec<_>>(),
        vec![pet.clone()],
        "the pet does not come back under today's owner handle, in thing scope: \
         {found_by_new:?}",
    );

    let found_by_old = graph::walk(mentioning, &query(was.as_str()))
        .await
        .expect("a thing-scope filter on a reference key must resolve");
    assert!(
        found_by_old.objects.is_empty(),
        "the pet still comes back under the owner's stale handle, in thing scope: \
         {found_by_old:?}",
    );
}

/// 🚨 **A rename, through the real verb, and every kind of reference a
/// reader resolves still resolves in the same call: a mention, a
/// reference-typed field value, an edge and a ref.**
///
/// **Through the served surface, not the test-only bypass.** Every case
/// above proves the READ side by staging a rename past the guard,
/// because no verb existed to produce one honestly. This is the one
/// case that calls `rename_entity` itself, so it is the proof that the
/// write side and the read side actually meet — four references, four
/// assertions, because a case covering one says nothing about the
/// others.
///
/// **Paired with a claim's own home**, both directions in the same read:
/// the renamed thing's own claim is found under its new handle AND its
/// stale one, and a handle that never existed still misses — resolving
/// a stale handle and refusing an unknown one are two different
/// mechanisms, and a case proving only the first cannot tell a working
/// redirect from a gate that answers everything.
pub async fn a_rename_moves_the_handle_and_every_reference_still_resolves<
    M: Memory + ?Sized,
    B: Memory + ?Sized,
>(
    mentioning: &M,
    bare: &B,
) {
    mentioning
        .declare_type(DeclaredType::new(
            "contract-rename-link",
            vec![Field::new("about", ValueType::Reference)],
        ))
        .await
        .expect("the type is declared");

    let author = EntityId::person("person:contract-rename-author");
    ensure(mentioning, &author).await;
    let was = EntityId("thing:contract-rename-was".into());
    mentioning
        .add_entity(NewEntity::new(
            was.clone(),
            "The Renamed Thing",
            "the roster",
        ))
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");

    let mut linking = NewFact::about(
        author.clone(),
        "mentioned @thing:contract-rename-was in the same breath",
        date(2026, 6, 1),
    );
    linking.edge = Some(Edge::new(EdgeShape::About, was.clone()));
    linking.refs = vec![was.clone()];
    linking.fields = [("about".to_string(), was.to_string())]
        .into_iter()
        .collect();
    let written = capture(mentioning, linking).await;

    // **What the store keeps, before anything moves.** A mention is
    // stored as the badge, never the handle, so "nothing was rewritten"
    // is proved by this staying byte-identical across the rename — not
    // by it still naming the old handle as text, which an edge or a ref
    // would but a mention never does.
    let stored_before = bare
        .recall(&author)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == written.id)
        .expect("the claim is there")
        .content;

    // The renamed thing's own claim — what proves "a claim's home" is
    // the fourth reference, not a repeat of the first three.
    let own = capture(
        mentioning,
        NewFact::about(was.clone(), "a claim on the thing itself", date(2026, 6, 1)),
    )
    .await;

    let now = EntityId("work:contract-rename-now".into());
    mentioning
        .rename_entity(&was, &now, None, date(2026, 6, 2), None)
        .await
        .expect("the rename lands")
        .written()
        .expect("nothing collides with the destination");

    let after = mentioning
        .recall(&author)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == written.id)
        .expect("the claim is there");
    assert!(
        after.content.contains(now.as_str()) && !after.content.contains(was.as_str()),
        "the mention did not follow the rename: {after:?}",
    );
    assert_eq!(
        after.edge.as_ref().map(|e| &e.object),
        Some(&now),
        "the edge did not follow the rename: {after:?}",
    );
    assert_eq!(
        after.refs,
        vec![now.clone()],
        "the ref did not follow the rename: {after:?}",
    );
    assert_eq!(
        after.fields.get("about").map(String::as_str),
        Some(now.as_str()),
        "the reference-typed field did not follow the rename: {after:?}",
    );

    // The claim's own home, both directions, in the same read as a
    // handle that never existed.
    let by_new_name = mentioning
        .recall(&now)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == own.id);
    assert!(
        by_new_name.is_some(),
        "the renamed thing's own claim did not resolve under its new handle",
    );
    let by_old_name = mentioning
        .recall(&was)
        .await
        .expect("a stale handle still resolves")
        .into_iter()
        .find(|f| f.id == own.id);
    assert!(
        by_old_name.is_some(),
        "the renamed thing's own claim did not resolve under its stale handle",
    );
    let never = EntityId("thing:contract-rename-never-existed".into());
    assert!(
        mentioning.recall(&never).await.is_err(),
        "a handle nothing ever answered to must still miss",
    );

    // 🚨 **The same three tiers, through `graph::walk` rather than the
    // trait method directly** (decision log 272) — `recall`'s served
    // handler and `capture`'s held-check both reach a subject through
    // `walk`, never through `Memory::recall`, so a case proving the
    // trait method resolves an old handle says nothing about what a real
    // caller sees.
    let walked_by_old_name = graph::walk(
        bare,
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(was.clone()),
                ..graph::Selection::default()
            },
            include: graph::Include {
                facts: true,
                prose: false,
                stood_for: false,
            },
            follow: None,
            history: None,
        },
    )
    .await
    .expect("a walk from a stale-but-renamed handle must still resolve");
    assert_eq!(
        walked_by_old_name.objects.first().map(|o| &o.entity.id),
        Some(&now),
        "the walk resolved the old handle to the wrong thing, or not at all",
    );
    let walked_never = graph::walk(
        bare,
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(never.clone()),
                ..graph::Selection::default()
            },
            include: graph::Include {
                facts: false,
                prose: false,
                stood_for: false,
            },
            follow: None,
            history: None,
        },
    )
    .await;
    assert!(
        walked_never.is_err(),
        "a walk from a handle nothing ever answered to must still miss: {walked_never:?}",
    );

    // Nothing was rewritten to do it: the stored form is byte-identical
    // to what it was before the rename.
    let stored_after = bare
        .recall(&author)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == written.id)
        .expect("the claim is there")
        .content;
    assert_eq!(
        stored_after, stored_before,
        "the stored mention changed, so this is find-and-replace rather than a pointer",
    );
}

/// 🚨 **An edit that touches only the content still leaves every pointer
/// resolvable after a later rename.**
///
/// `update_fact` reads a record already served under today's handles, so
/// its edge, its ref and the claim it derives from all arrive in handle
/// form before the patch ever runs. Writing that same record back without
/// lowering them first stores the handle where the badge belongs — a
/// defect invisible right now, because the handle still resolves, and only
/// surfaces the day the pointed-to thing is renamed and nothing points at
/// it any more.
///
/// **The edit is the whole point of this case**: [`a_rename_moves_the_handle_and_every_reference_still_resolves`]
/// already proves a rename is followed by a record nothing has written
/// since capture. This proves the same thing survives an edit landing in
/// between, which a write-back that lowers conditionally does not.
pub async fn an_edit_that_touches_only_the_content_still_follows_a_later_rename<
    M: Memory + ?Sized,
>(
    store: &M,
) {
    let author = EntityId::person("person:contract-edit-pointer-author");
    ensure(store, &author).await;
    let was = EntityId("thing:contract-edit-pointer-was".into());
    add(
        store,
        NewEntity::new(was.clone(), "The Pointer Edit Target", "the roster"),
    )
    .await;

    let mut linking = NewFact::about(
        author.clone(),
        "mentions the pointer target before any edit",
        date(2026, 6, 10),
    );
    linking.edge = Some(Edge::new(EdgeShape::About, was.clone()));
    linking.refs = vec![was.clone()];
    let written = capture(store, linking).await;

    store
        .update_fact(
            &written.address(),
            FactPatch {
                content: Some("only the words changed".into()),
                ..Default::default()
            },
        )
        .await
        .expect("update_fact should succeed")
        .written()
        .expect("nothing blocks a content-only edit");

    let now = EntityId("work:contract-edit-pointer-now".into());
    store
        .rename_entity(&was, &now, None, date(2026, 6, 11), None)
        .await
        .expect("the rename lands")
        .written()
        .expect("nothing collides with the destination");

    let after = store
        .recall(&author)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == written.id)
        .expect("the claim is there");
    assert_eq!(
        after.edge.as_ref().map(|e| &e.object),
        Some(&now),
        "the edge did not follow the rename after a content-only edit: {after:?}",
    );
    assert_eq!(
        after.refs,
        vec![now],
        "the ref did not follow the rename after a content-only edit: {after:?}",
    );
}

/// 🚨 **A retraction still leaves every pointer resolvable after a later
/// rename.**
///
/// The same hazard as
/// [`an_edit_that_touches_only_the_content_still_follows_a_later_rename`],
/// over `retract` instead of `update_fact`: a retraction reads the served
/// record too, and rewrites the row it retracts from the rest of it. A
/// write-back that lowers only `home`, `subject` and `derived_from` leaves
/// the retracted row's own edge and ref in handle form, where they stand
/// until the pointed-to thing is renamed.
pub async fn a_retraction_still_follows_a_later_rename<M: Memory + ?Sized>(store: &M) {
    let author = EntityId::person("person:contract-retract-pointer-author");
    ensure(store, &author).await;
    let was = EntityId("thing:contract-retract-pointer-was".into());
    add(
        store,
        NewEntity::new(was.clone(), "The Pointer Retraction Target", "the roster"),
    )
    .await;

    let mut linking = NewFact::about(
        author.clone(),
        "mentions the pointer target before any retraction",
        date(2026, 6, 12),
    );
    linking.edge = Some(Edge::new(EdgeShape::About, was.clone()));
    linking.refs = vec![was.clone()];
    let written = capture(store, linking).await;

    store
        .retract(&written.address(), Some("no longer so"), date(2026, 6, 13))
        .await
        .expect("the retraction lands");

    let now = EntityId("work:contract-retract-pointer-now".into());
    store
        .rename_entity(&was, &now, None, date(2026, 6, 14), None)
        .await
        .expect("the rename lands")
        .written()
        .expect("nothing collides with the destination");

    let after = store
        .recall(&author)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == written.id)
        .expect("the retracted claim is still there");
    assert_eq!(
        after.status,
        FactStatus::Archived,
        "the retraction did not land: {after:?}",
    );
    assert_eq!(
        after.edge.as_ref().map(|e| &e.object),
        Some(&now),
        "the retracted row's edge did not follow the rename: {after:?}",
    );
    assert_eq!(
        after.refs,
        vec![now],
        "the retracted row's ref did not follow the rename: {after:?}",
    );
}

/// 🚨 **A former handle carries more than one event, and a lookup on it
/// resolves to the newest.**
///
/// Nothing reserves a handle a rename vacates: a second thing can claim it
/// immediately, and renaming THAT thing away writes a second event under
/// the same former handle. That is not a contrived sequence — it is what
/// ordinary use produces the moment a vacated handle is reused, since
/// nothing stops it.
pub async fn a_former_handle_reused_after_a_rename_resolves_to_the_newest_event<
    M: Memory + ?Sized,
    B: Memory + ?Sized,
>(
    store: &M,
    bare: &B,
) {
    let shared = EntityId("thing:contract-reused-former-handle".into());
    add(
        store,
        NewEntity::new(shared.clone(), "The First Claim", "the roster"),
    )
    .await;
    let first_gone = EntityId("work:contract-reused-former-first-gone".into());
    store
        .rename_entity(&shared, &first_gone, None, date(2026, 6, 20), None)
        .await
        .expect("the first rename lands")
        .written()
        .expect("nothing collides with the destination");

    add(
        store,
        NewEntity::new(shared.clone(), "The Second Claim", "the roster"),
    )
    .await;
    let second_gone = EntityId("work:contract-reused-former-second-gone".into());
    store
        .rename_entity(&shared, &second_gone, None, date(2026, 6, 21), None)
        .await
        .expect("the second rename of the reused handle lands")
        .written()
        .expect("nothing collides with the destination");

    let walked = graph::walk(
        bare,
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(shared.clone()),
                ..graph::Selection::default()
            },
            include: graph::Include {
                facts: false,
                prose: false,
                stood_for: false,
            },
            follow: None,
            history: None,
        },
    )
    .await
    .expect("a walk from a former handle carrying two events must still resolve");
    assert_eq!(
        walked.objects.first().map(|o| &o.entity.id),
        Some(&second_gone),
        "the former handle resolved to the first thing that ever wore it, not the newest",
    );
}

/// 🚨 **A retype and a reparent are each their own case, not a slug
/// change in disguise.**
///
/// The handle is one string, so this verb reaches all three the same
/// way — but a case that only ever moved the slug half would say
/// nothing about whether the kind half and the parent pointer actually
/// move too. `children()` is the proof that matters for the parent: a
/// fix that only patched `Entity::parent` and left the reverse lookup
/// reading a stale value would still lose a session asking what is
/// under a thing now.
pub async fn a_retype_and_a_reparent_are_each_a_rename<M: Memory + ?Sized>(store: &M) {
    let parent = EntityId("person:contract-retype-parent".into());
    store
        .add_entity(NewEntity::new(
            parent.clone(),
            "Contract Retype Parent",
            "the roster",
        ))
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");

    let was = EntityId("thing:contract-retype-was".into());
    store
        .add_entity(NewEntity::new(
            was.clone(),
            "Contract Retype Was",
            "the roster",
        ))
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");

    let child = EntityId("thing:contract-retype-child".into());
    store
        .add_entity(NewEntity {
            parent: Some(was.clone()),
            ..NewEntity::new(child.clone(), "Contract Retype Child", "the roster")
        })
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");

    // Both at once: a different kind AND a different parent, in the one
    // call — proving neither rides on the other.
    let now = EntityId("work:contract-retype-now".into());
    let renamed = store
        .rename_entity(&was, &now, Some(parent.clone()), date(2026, 6, 3), None)
        .await
        .expect("the rename lands")
        .written()
        .expect("nothing collides with the destination");
    assert_eq!(
        renamed.kind,
        EntityKind::WORK,
        "the retype did not land: {renamed:?}",
    );
    assert_eq!(
        renamed.parent.as_ref(),
        Some(&parent),
        "the reparent did not land: {renamed:?}",
    );

    let read_back = store
        .list_entities(None)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|e| e.id == now)
        .expect("the renamed row is there");
    assert_eq!(read_back.kind, EntityKind::WORK);
    assert_eq!(read_back.parent.as_ref(), Some(&parent));

    // The reverse lookup — not only the field on the child, which a
    // half-built fix could patch and leave the walk reading the old
    // value.
    let kids = store
        .children(&parent)
        .await
        .expect("children reads under the current handle");
    assert!(
        kids.contains(&now),
        "the renamed-and-reparented thing is not among its new parent's children: {kids:?}",
    );

    // **The other repoint in the same write**: `child` named `was` as
    // its own parent, so it follows to `now` — the mechanism
    // `entity.parent` resolve-on-read already proved through the test
    // harness, re-proved here through the real verb.
    let grandkids = store
        .children(&now)
        .await
        .expect("children reads under the current handle");
    assert_eq!(
        grandkids,
        vec![child],
        "a thing parented on the renamed entity did not follow it: {grandkids:?}",
    );

    // 🚨 **The old handle is a real answer, not a miss** (decision log
    // 272): `was` still names the thing it always did, just under a
    // handle it no longer wears, so asking what is under it answers
    // exactly as asking under `now` does.
    let kids_by_old_name = store
        .children(&was)
        .await
        .expect("a stale-but-renamed handle must still resolve");
    assert_eq!(
        kids_by_old_name, grandkids,
        "children() under the old handle did not match children() under the new one",
    );
}

/// 🚨 **A stale address resolves to the SAME claim after a rename, even
/// once new claims exist under the new handle.**
///
/// **The trap this closes**: a local id is minted per storage key. If
/// that key were the handle, a renamed thing's new handle would start
/// minting at `f1` again, and an address built before the rename would
/// silently resolve to whatever new claim now sits at that number — a
/// confident wrong answer, not a miss. Storing the badge is what keeps
/// minting scoped to the one thing across every name it has worn, so
/// this never happens.
///
/// **Paired in the same read**: a handle that never held this local id
/// still misses, so the claim above is that resolution works and not
/// that every address is accepted.
pub async fn a_stale_address_resolves_to_the_same_claim_after_a_rename<M: Memory + ?Sized>(
    store: &M,
    rehandles: &dyn Rehandles,
) {
    let was = EntityId("person:contract-address-restart-was".into());
    let now = EntityId("work:contract-address-restart-now".into());
    match store
        .add_entity(NewEntity::new(
            was.clone(),
            "The Numbered One",
            "the roster",
        ))
        .await
        .expect("the fixture is written")
    {
        Guarded::Written(_) => {}
        blocked => panic!("nothing collides with it: {blocked:?}"),
    }
    let badge = store
        .list_entities(None)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|e| e.id == was)
        .expect("the fixture is there")
        .badge
        .expect("a written row wears a badge");

    let first = capture(
        store,
        NewFact::about(
            was.clone(),
            "the first claim under the old name",
            date(2026, 5, 1),
        ),
    )
    .await;
    let stale_address = first.address();

    rehandles.rehandle(&was, &now).await;
    rehandles
        .note_former_handle(FormerHandle {
            former: was.clone(),
            badge,
            changed_at: date(2026, 5, 2),
        })
        .await;

    // A claim written under the new handle, after the rename — minted at
    // whatever local id comes next for this thing's own history, which
    // is exactly what proves the scope did not restart.
    capture(
        store,
        NewFact::about(
            now.clone(),
            "a claim written after the rename",
            date(2026, 5, 3),
        ),
    )
    .await;

    let chain = store
        .claim_history(&stale_address)
        .await
        .unwrap_or_else(|e| panic!("the address minted before the rename must still resolve: {e}"));
    assert_eq!(
        chain.last().map(|w| w.content.as_str()),
        Some("the first claim under the old name"),
        "a stale address resolved to some other claim than the one it always named",
    );

    let never = FactAddress::new(was, FactId("f99".into()));
    assert!(
        store.claim_history(&never).await.is_err(),
        "a local id this thing never held must still miss, whichever of its handles is asked",
    );
}

/// 🚨 **An edit through a stale address lands on the record it always
/// named — it does not fork it onto a second key.**
///
/// **The hazard this closes**: an edit reads the addressed row through
/// the assembler that resolves a badge to today's handle, then writes it
/// back. If that write used the resolved HANDLE rather than the storage
/// key it was read under, the row would be filed under a plain string
/// nothing else looks up — the edit silently lost, the original content
/// standing untouched under the key every other read still resolves to.
/// **That is why the read here is the content, not merely success**: a
/// build with this hazard answers `Written` and changes nothing a reader
/// ever sees.
///
/// **Paired in one read**: a local id this thing never held still misses,
/// under either handle it has worn — the claim above is that resolution
/// works, not that every address is accepted.
pub async fn an_edit_through_a_stale_address_reaches_the_record_it_always_named<
    M: Memory + ?Sized,
>(
    store: &M,
    rehandles: &dyn Rehandles,
) {
    let was = EntityId("person:contract-stale-edit-was".into());
    let now = EntityId("work:contract-stale-edit-now".into());
    store
        .add_entity(NewEntity::new(was.clone(), "Before The Edit", "the roster"))
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");
    let badge = store
        .list_entities(None)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|e| e.id == was)
        .expect("the fixture is there")
        .badge
        .expect("a written row wears a badge");

    let written = capture(
        store,
        NewFact::about(was.clone(), "before the rename", date(2026, 5, 10)),
    )
    .await;
    let stale_address = written.address();

    rehandles.rehandle(&was, &now).await;
    rehandles
        .note_former_handle(FormerHandle {
            former: was.clone(),
            badge,
            changed_at: date(2026, 5, 11),
        })
        .await;

    store
        .update_fact(
            &stale_address,
            FactPatch {
                content: Some("edited through the stale address".into()),
                ..Default::default()
            },
        )
        .await
        .expect("the edit lands")
        .written()
        .expect("nothing blocks it");

    let held = store
        .recall(&now)
        .await
        .expect("the store answers under the current handle");
    assert_eq!(
        held.len(),
        1,
        "the edit through a stale address left more or fewer than the one claim this thing \
         has: {held:?}",
    );
    assert_eq!(
        held[0].content, "edited through the stale address",
        "the edit did not reach the record its stale address named — it is lost rather \
         than merely late",
    );

    let never = FactAddress::new(now, FactId("f99".into()));
    assert!(
        store
            .update_fact(
                &never,
                FactPatch {
                    content: Some("must not land".into()),
                    ..Default::default()
                },
            )
            .await
            .is_err(),
        "a local id this thing never held must still miss, whichever of its handles is asked",
    );
}

/// 🚨 **A retraction through a stale address retracts the record it
/// always named — it does not fork it onto a second key.**
///
/// The same hazard [`an_edit_through_a_stale_address_reaches_the_record_it_always_named`]
/// closes for `update_fact`, over `retract` instead: a write-back under
/// the resolved handle rather than the storage key would file the
/// retraction under a row nothing resolves to, leaving the original
/// standing as if nothing had been taken back.
pub async fn a_retraction_through_a_stale_address_reaches_the_record_it_always_named<
    M: Memory + ?Sized,
>(
    store: &M,
    rehandles: &dyn Rehandles,
) {
    let was = EntityId("person:contract-stale-retract-was".into());
    let now = EntityId("work:contract-stale-retract-now".into());
    store
        .add_entity(NewEntity::new(
            was.clone(),
            "Before The Retraction",
            "the roster",
        ))
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");
    let badge = store
        .list_entities(None)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|e| e.id == was)
        .expect("the fixture is there")
        .badge
        .expect("a written row wears a badge");

    let written = capture(
        store,
        NewFact::about(was.clone(), "stands until retracted", date(2026, 5, 12)),
    )
    .await;
    let stale_address = written.address();

    rehandles.rehandle(&was, &now).await;
    rehandles
        .note_former_handle(FormerHandle {
            former: was.clone(),
            badge,
            changed_at: date(2026, 5, 13),
        })
        .await;

    store
        .retract(&stale_address, Some("no longer so"), date(2026, 5, 14))
        .await
        .expect("the retraction lands");

    let held = store
        .recall(&now)
        .await
        .expect("the store answers under the current handle");
    let original = held
        .iter()
        .find(|f| f.id == written.id)
        .expect("the original claim is still there, retracted or not");
    assert_eq!(
        original.status,
        FactStatus::Archived,
        "the retraction through a stale address did not reach the record it always named — \
         it still stands: {original:?}",
    );

    let never = FactAddress::new(now, FactId("f99".into()));
    assert!(
        store
            .retract(&never, None, date(2026, 5, 14))
            .await
            .is_err(),
        "a local id this thing never held must still miss, whichever of its handles is asked",
    );
}

/// 🚨 **A link that leads nowhere and text that was never a link render
/// differently, and neither renders bare.**
///
/// They are told apart because they are STORED differently, never by how
/// they are dressed: one is a badge nothing wears, the other is a handle
/// nobody answers to. **Both in one read**, because a build that marked
/// everything the same way would satisfy either half alone.
///
/// Written through the bare store, which is how both really arise: text
/// that predates this layer, and a row that left the store afterwards.
pub async fn a_dead_link_and_text_that_was_never_a_link_read_differently<
    M: Memory + ?Sized,
    B: Memory + ?Sized,
>(
    mentioning: &M,
    bare: &B,
) {
    let author = EntityId::person("person:contract-mention-broken");
    ensure(bare, &author).await;
    let dead = format!("{}zzzzzz", mention::MARK);
    let never = "@person:contract-mention-nobody";
    let written = capture(
        bare,
        NewFact::about(
            author.clone(),
            format!("the note pointed at {dead} and at {never}"),
            date(2026, 4, 18),
        ),
    )
    .await;

    let read = read_claim(mentioning, &author, &written.id).await;
    assert!(
        read.contains("zzzzzz"),
        "the dead link keeps its badge, so a person has something to look for: {read}",
    );
    assert!(
        read.contains(never),
        "the text that was never a link is still the words the author wrote: {read}",
    );
    // **Neither is served as it was stored**, which is what *never bare*
    // means: a broken pointer that reads as ordinary text reads as a
    // complete sentence.
    assert!(
        !read.contains(&format!("{dead} and")),
        "the dead link was served bare: {read}",
    );
    assert!(
        !read.contains(&format!("{never}\""))
            && read != stored_of(bare, &author, &written.id).await,
        "the never-a-link text was served bare: {read}",
    );
    // ⭐ **And the two are marked differently**, which is the whole claim:
    // a reader can tell *this pointed somewhere and the thing is gone* from
    // *this was never a pointer at all*.
    let after_dead = read
        .split_once("zzzzzz")
        .expect("the dead link is in the answer")
        .1;
    let after_never = read
        .split_once(never)
        .expect("the never-a-link text is in the answer")
        .1;
    assert_ne!(
        mark_at(after_dead),
        mark_at(after_never),
        "the two render identically, so nothing tells them apart: {read}",
    );
}

/// The bracketed note a render leaves after a mention it could not make a
/// link, or the empty string where it left none.
fn mark_at(rest: &str) -> &str {
    let rest = rest.trim_start();
    match rest.starts_with('(') {
        true => rest.split_once(')').map_or("", |(mark, _)| mark),
        false => "",
    }
}

async fn read_claim<M: Memory + ?Sized>(store: &M, subject: &EntityId, id: &FactId) -> String {
    stored_of(store, subject, id).await
}

async fn stored_of<M: Memory + ?Sized>(store: &M, subject: &EntityId, id: &FactId) -> String {
    store
        .recall(subject)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| &f.id == id)
        .expect("the claim is there")
        .content
}

/// **The fake's way of moving a handle**: rewrite the row in place, badge
/// and all, which is what a hand edit outside jojobot does to a real store.
pub struct FakeRehandles(pub std::sync::Arc<InMemoryMemory>);

#[async_trait::async_trait]
impl Rehandles for FakeRehandles {
    async fn rehandle(&self, from: &EntityId, to: &EntityId) {
        self.0.rehandle_past_the_guard(from, to);
    }
    async fn note_former_handle(&self, event: FormerHandle) {
        self.0.former_handle_past_the_guard(event);
    }
}

/// 🚨 **A mention naming nothing is refused, and nothing is written.**
///
/// The rule an edge's object already faces and a record's refs already
/// face: every entity a claim names must already exist. A mention treated
/// more loosely would be the same act guarded in one column and waved
/// through in the next — and the place it is waved through is the one place
/// nobody looks, because it reads as a sentence.
///
/// **Paired with the write that must still land**, or a build refusing
/// every claim with an `@` in it passes the first half.
pub async fn a_mention_naming_nothing_is_refused_and_writes_nothing<
    M: Memory + ?Sized,
    B: Memory + ?Sized,
>(
    mentioning: &M,
    bare: &B,
) {
    let author = EntityId::person("person:contract-mention-typist");
    ensure(mentioning, &author).await;
    let before = bare.recall(&author).await.expect("the store answers").len();

    // **The claim and the nuance are two lines of the same guard**, so each
    // is refused on its own rather than on the other.
    for fact in [
        NewFact::about(
            author.clone(),
            "the note named @person:contract-mention-nobody",
            date(2026, 4, 18),
        ),
        NewFact {
            details: Some("named @person:contract-mention-nobody underneath".into()),
            ..NewFact::about(author.clone(), "the note said nothing", date(2026, 4, 18))
        },
    ] {
        let refused = mentioning
            .capture(fact)
            .await
            .expect("a refusal is an answer rather than an error");
        match &refused {
            Guarded::Blocked { attempted, .. } => assert_eq!(
                attempted.as_str(),
                "person:contract-mention-nobody",
                "the refusal names the mention that could not be followed",
            ),
            Guarded::Written(written) => panic!(
                "a claim naming something that is not there was stored: {}",
                written.content
            ),
        }
    }
    assert_eq!(
        bare.recall(&author).await.expect("the store answers").len(),
        before,
        "the refused write left a record behind",
    );

    // **An EDIT names entities exactly as a capture does**, and it is the
    // likelier of the two: a correction is where somebody rewrites the
    // sentence. Its own assertion, because it is its own screen.
    let standing = capture(
        mentioning,
        NewFact::about(author.clone(), "kept the books", date(2026, 4, 18)),
    )
    .await;
    // **The claim and the nuance are two lines of the same guard**, so
    // each is refused on its own rather than on the other.
    for patch in [
        FactPatch {
            content: Some("kept the books for @person:contract-mention-nobody".into()),
            ..Default::default()
        },
        FactPatch {
            details: Some("for @person:contract-mention-nobody".into()),
            ..Default::default()
        },
    ] {
        let refused = mentioning
            .update_fact(&standing.address(), patch)
            .await
            .expect("a refusal is an answer rather than an error");
        match &refused {
            Guarded::Blocked { attempted, .. } => assert_eq!(
                attempted.as_str(),
                "person:contract-mention-nobody",
                "the refused edit names the mention that could not be followed",
            ),
            Guarded::Written(written) => panic!(
                "an edit naming something that is not there was stored: {}",
                written.content
            ),
        }
    }
    assert_eq!(
        stored_of(bare, &author, &standing.id).await,
        "kept the books",
        "the refused edit rewrote the claim anyway",
    );

    // …and a claim mentioning something that IS there still lands.
    let landed = capture(
        mentioning,
        NewFact::about(
            author.clone(),
            "the note named @person:contract-mention-typist",
            date(2026, 4, 18),
        ),
    )
    .await;
    assert!(
        landed.content.contains("person:contract-mention-typist"),
        "a mention of something that exists is written and read back: {}",
        landed.content,
    );
}

/// 🚨 **Every read that returns text serves handles, and no read serves a
/// badge.**
///
/// A layer that renders text has to cover every read that returns any, and
/// a read added later inherits nothing — so the case walks them rather than
/// trusting that one implies the rest. **A badge reaching a caller is the
/// failure**: it is not addressable, nothing accepts it back, and a client
/// that stored one would be holding a name jojobot has no verb for.
pub async fn no_read_serves_a_badge_and_every_one_serves_the_handle<M: Memory + ?Sized>(store: &M) {
    let author = EntityId::person("person:contract-mention-bookkeeper");
    let named = EntityId("place:contract-mention-inn".into());
    ensure(store, &author).await;
    store
        .add_entity(NewEntity::new(named.clone(), "The Inn", "the roster"))
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");

    // A page, a claim, a correction, a note on a key read two ways, and a
    // retraction — every door text leaves this store by.
    let page = store
        .set_prose(&author, "keeps the books at @place:contract-mention-inn")
        .await
        .expect("the page is written");
    let written = capture(
        store,
        NewFact {
            details: Some("first heard at @place:contract-mention-inn".into()),
            fields: [("tabs".to_string(), "3".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                author.clone(),
                "was owed money by @place:contract-mention-inn",
                date(2026, 4, 18),
            )
        },
    )
    .await;
    store
        .update_fact(
            &written.address(),
            FactPatch {
                content: Some("was owed nothing by @place:contract-mention-inn".into()),
                ..Default::default()
            },
        )
        .await
        .expect("the correction lands")
        .written()
        .expect("nothing blocks it");

    let mut served: Vec<String> = vec![page];
    // **A claim's own two texts, from the read a caller makes.** The
    // nuance under a claim is text like the claim, and it leaves the store
    // by a different line of the same function — so a case reading only
    // the headline proves half an arm.
    for fact in store.recall(&author).await.expect("the store answers") {
        served.push(fact.content);
        served.extend(fact.details);
    }
    served.push(
        store
            .scan_entity(&author)
            .await
            .expect("the scan reads")
            .expect("the author is a document")
            .prose,
    );
    for write in store
        .claim_history(&written.address())
        .await
        .expect("the chain reads")
    {
        served.push(write.content);
        served.extend(write.details);
    }
    // A key's history carries the note its record carried, which is the
    // same text under a different name.
    for write in store
        .history(&author, "tabs")
        .await
        .expect("the writes read")
    {
        served.extend(write.note);
    }
    // **A key's backing carries the same note, through a different door.**
    // `history` and `backing` answer different questions about one key —
    // every write in order, and what the key holds now — but both carry the
    // record's note, so both have to render it.
    for held in store
        .backing(&author)
        .await
        .expect("the backing reads")
        .into_values()
    {
        served.extend(held.note);
    }
    // What points here, and what was built on what — two reads that hand
    // back claims written on something else.
    // **A key holding the handle**, which is what this read matches: it
    // answers who points here through a field rather than through an edge.
    let pointing = capture(
        store,
        NewFact {
            fields: [("owed_by".to_string(), author.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                named.clone(),
                "keeps a tab for @person:contract-mention-bookkeeper",
                date(2026, 4, 18),
            )
        },
    )
    .await;
    let pointed: Vec<_> = store
        .referring_to(&author)
        .await
        .expect("what points here reads");
    assert!(
        !pointed.is_empty(),
        "nothing points here, so this read proves nothing about how it renders",
    );
    served.extend(pointed.into_iter().map(|fact| fact.content));
    capture(
        store,
        NewFact {
            derived_from: Some(pointing.address()),
            ..NewFact::about(
                named.clone(),
                "so the tab at @place:contract-mention-inn is still open",
                date(2026, 4, 19),
            )
        },
    )
    .await;
    for fact in store
        .built_on(&pointing.address())
        .await
        .expect("the lineage reads")
    {
        served.push(fact.content);
    }
    let taken_back = store
        .retract(
            &written.address(),
            Some("nobody at @place:contract-mention-inn remembers it"),
            date(2026, 4, 19),
        )
        .await
        .expect("the retraction lands");
    served.push(taken_back.record.content);
    served.push(taken_back.retracted.content);

    for text in &served {
        assert!(
            !text.contains(mention::MARK),
            "a badge reached a caller, which is a name nothing here accepts back: {text}",
        );
    }
    assert!(
        served
            .iter()
            .filter(|text| text.contains(named.as_str()))
            .count()
            >= 12,
        "some read served text with the mention taken out of it: {served:?}",
    );
}

/// 🚨 **An account jojobot writes from a caller's reason carries that
/// caller's mentions**, stored as a pointer and read back as a handle.
///
/// A fold and a retraction both write one, and both outlive everybody who
/// remembers the act — which is exactly the text a stale handle spoils.
/// **Both halves in one read**: the store keeps no handle, and the answer
/// carries one.
pub async fn an_account_written_from_a_reason_stores_its_mentions<
    M: Memory + ?Sized,
    B: Memory + ?Sized,
>(
    mentioning: &M,
    bare: &B,
) {
    let survivor = EntityId::person("person:contract-mention-kept");
    let folded = EntityId::person("person:contract-mention-gone");
    let named = EntityId("org:contract-mention-guild".into());
    for (handle, name) in [
        (&survivor, "The One Kept"),
        (&folded, "The One Folded"),
        (&named, "The Guild"),
    ] {
        mentioning
            .add_entity(NewEntity::new(handle.clone(), name, "the roster"))
            .await
            .expect("the fixture is written")
            .written()
            .expect("nothing collides with it");
    }

    let done = mentioning
        .merge(
            &folded,
            &survivor,
            Some("both were the same member of @org:contract-mention-guild"),
            date(2026, 4, 18),
        )
        .await
        .expect("the fold lands");
    assert!(
        done.record.content.contains(named.as_str())
            || done
                .record
                .details
                .as_deref()
                .is_some_and(|d| d.contains(named.as_str())),
        "the account handed back carries no handle: {:?}",
        done.record,
    );

    let kept = bare
        .recall(&survivor)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == done.record.id)
        .expect("the account is on the survivor");
    let stored = format!("{}{}", kept.content, kept.details.unwrap_or_default());
    assert!(
        !stored.contains(named.as_str()) && stored.contains(mention::MARK),
        "the account was stored with the handle in it, so a rename spoils the one record \
         that outlives everybody who remembers the fold: {stored}",
    );

    // **A retraction writes the same kind of account**, so its reason is
    // asked the same question rather than trusted to follow.
    let claim = capture(
        mentioning,
        NewFact::about(survivor.clone(), "paid the subscription", date(2026, 4, 18)),
    )
    .await;
    let taken_back = mentioning
        .retract(
            &claim.address(),
            Some("@org:contract-mention-guild says otherwise"),
            date(2026, 4, 19),
        )
        .await
        .expect("the retraction lands");
    assert!(
        taken_back
            .record
            .details
            .as_deref()
            .is_some_and(|d| d.contains(named.as_str()))
            || taken_back.record.content.contains(named.as_str()),
        "the retraction handed back carries no handle: {:?}",
        taken_back.record,
    );
    let kept = bare
        .recall(&survivor)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == taken_back.record.id)
        .expect("the retraction is on the thing");
    let stored = format!("{}{}", kept.content, kept.details.unwrap_or_default());
    assert!(
        !stored.contains(named.as_str()) && stored.contains(mention::MARK),
        "the retraction was stored with the handle in it: {stored}",
    );
}

/// The mention contract, over a store that resolves and renders them and
/// the same store read bare.
pub async fn run_all_mentioning<M: Memory + ?Sized, B: Memory + ?Sized>(
    mentioning: &M,
    bare: &B,
    rehandles: &dyn Rehandles,
) {
    a_mention_is_stored_as_a_badge_and_read_back_as_a_handle(mentioning, bare).await;
    a_mention_follows_a_thing_that_is_rehandled(mentioning, bare, rehandles).await;
    a_dead_link_and_text_that_was_never_a_link_read_differently(mentioning, bare).await;
    a_mention_naming_nothing_is_refused_and_writes_nothing(mentioning, bare).await;
    no_read_serves_a_badge_and_every_one_serves_the_handle(mentioning).await;
    an_account_written_from_a_reason_stores_its_mentions(mentioning, bare).await;
    a_stale_handle_resolves_through_its_rename_history(bare, rehandles).await;
    a_childs_parent_pointer_follows_a_rename(bare, rehandles).await;
    an_edge_follows_a_thing_that_is_rehandled(mentioning, bare, rehandles).await;
    a_ref_follows_a_thing_that_is_rehandled(mentioning, bare, rehandles).await;
    a_reference_typed_field_value_resolves_in_thing_scope_after_a_rename(mentioning).await;
    a_rename_moves_the_handle_and_every_reference_still_resolves(mentioning, bare).await;
    an_edit_that_touches_only_the_content_still_follows_a_later_rename(mentioning).await;
    a_retraction_still_follows_a_later_rename(mentioning).await;
    a_former_handle_reused_after_a_rename_resolves_to_the_newest_event(mentioning, bare).await;
    a_retype_and_a_reparent_are_each_a_rename(bare).await;
    a_stale_address_resolves_to_the_same_claim_after_a_rename(bare, rehandles).await;
    an_edit_through_a_stale_address_reaches_the_record_it_always_named(bare, rehandles).await;
    a_retraction_through_a_stale_address_reaches_the_record_it_always_named(bare, rehandles).await;
}
