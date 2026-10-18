//! **The homepage and the page of a kind**, both listings of what the store
//! holds.
//!
//! The homepage is the root of the filesystem: a box for each kind that has
//! members (how many, the three written last, a link to all of them), the box of
//! what is due, and the top level below them. **jojobot ranks nothing here**: the
//! only order is when a thing was last written, or its handle.
//!
//! A thing's **last write** is the moment jojobot took in the newest claim on
//! it. It is read for every thing in one pass of the store's scan, so a page of
//! many things costs one read and not one for each.

use std::collections::{BTreeMap, HashMap};

use axum::{
    extract::{Path, Query, State},
    response::Response,
};
use jiff::Timestamp;
use jojobot_domain::memory::{Entity, EntityId, MemoryError};

use crate::AppState;
use crate::ui::{org, pages, shell, tree};

/// Where the page of a kind lives: `/ui/kind/<kind>/`. It is no entity's path,
/// so the gate knows the prefix ([`crate::ui::require_browser`]).
pub const KIND_PATH: &str = "/ui/kind/";

/// The most a kind's box shows of its members.
const SHOWN: usize = 3;

/// The viewer's choice of order for a listing: `?sort=name` (the default) or
/// `?sort=written`.
#[derive(Debug, Default, serde::Deserialize)]
pub struct Sort {
    #[serde(default)]
    sort: Option<String>,
}

impl Sort {
    fn by_write(&self) -> bool {
        self.sort.as_deref() == Some("written")
    }
}

/// **When each thing was last written**: the newest moment any claim on it was
/// taken in. A thing nobody has written a claim on has none.
async fn last_writes(state: &AppState) -> Result<HashMap<EntityId, Timestamp>, MemoryError> {
    Ok(state
        .memory
        .scan()
        .await?
        .into_iter()
        .filter_map(|doc| {
            let newest = doc.facts.iter().filter_map(|fact| fact.inserted_at).max()?;
            Some((doc.entity?.id, newest))
        })
        .collect())
}

/// **The things that stand**: not archived, not folded into another. Every
/// count, box and listing here is made of these and no others.
fn standing(entities: &[Entity]) -> Vec<&Entity> {
    entities.iter().filter(|e| e.browsable()).collect()
}

/// **A moment as the day it fell on in the instance's zone**, the day the rest of
/// the operator's window reads: the frame is read once for a page, and the clock
/// turns each moment into a day in it.
type DayOf<'a> = &'a dyn Fn(Timestamp) -> String;

async fn day_of_in_the_instance_zone(state: &AppState) -> impl Fn(Timestamp) -> String + use<> {
    let frame = jojobot_mcp::orientation::instance_zone::unzoned_frame(&*state.memory).await;
    let clock = state.clock;
    move |moment| clock.day_of(moment, &frame).to_string()
}

/// **Things in the order a listing is asked for**: by handle, or the thing
/// written last first with ties in handle order.
fn ordered<'a>(
    mut things: Vec<&'a Entity>,
    written: &HashMap<EntityId, Timestamp>,
    by_write: bool,
) -> Vec<&'a Entity> {
    things.sort_by(|a, b| {
        if by_write {
            written
                .get(&b.id)
                .cmp(&written.get(&a.id))
                .then_with(|| a.id.cmp(&b.id))
        } else {
            a.id.cmp(&b.id)
        }
    });
    things
}

/// **The listing table**: path, kind, how many things sit directly inside, and
/// the day of the last write.
fn listing(
    id: &str,
    things: &[&Entity],
    inside: &HashMap<&EntityId, usize>,
    written: &HashMap<EntityId, Timestamp>,
    by_id: &HashMap<&EntityId, &Entity>,
    day_of: DayOf<'_>,
) -> String {
    let rows: Vec<Vec<(&str, String)>> = things
        .iter()
        .map(|entity| {
            vec![
                (
                    "p",
                    format!(
                        "<a href=\"{}\">{}/</a>",
                        pages::escape(&tree::canonical_path(entity, by_id)),
                        pages::escape(entity.id.as_str())
                    ),
                ),
                ("k", pages::escape(entity.kind.as_token())),
                (
                    "n",
                    inside
                        .get(&entity.id)
                        .map_or("—".to_string(), usize::to_string),
                ),
                (
                    "d",
                    written
                        .get(&entity.id)
                        .map_or("—".to_string(), |m| day_of(*m)),
                ),
            ]
        })
        .collect();
    shell::row_table(id, "fs", &["path", "kind", "inside", "last write"], &rows)
}

/// How many things stand directly inside each thing.
fn inside_of<'a>(standing: &[&'a Entity]) -> HashMap<&'a EntityId, usize> {
    let mut inside: HashMap<&EntityId, usize> = HashMap::new();
    for entity in standing {
        if let Some(parent) = &entity.parent {
            *inside.entry(parent).or_default() += 1;
        }
    }
    inside
}

/// The sort links for a listing's heading.
fn sort_links(path: &str) -> String {
    format!(
        "<a href=\"{path}?sort=name\">sort by name</a> · <a href=\"{path}?sort=written\">by last write</a>"
    )
}

/// **One kind's box**: its name, how many stand, the three written last and the
/// way to all of them.
fn kind_box(
    kind: &str,
    members: &[&Entity],
    written: &HashMap<EntityId, Timestamp>,
    by_id: &HashMap<&EntityId, &Entity>,
    day_of: DayOf<'_>,
) -> String {
    let newest = ordered(members.to_vec(), written, true);
    let mut list = String::from("<ul>\n");
    for entity in newest.iter().take(SHOWN) {
        list.push_str(&format!(
            "<li><span class=\"t\"><a href=\"{}\">{}</a></span><span class=\"m\">{}</span></li>\n",
            pages::escape(&tree::canonical_path(entity, by_id)),
            pages::escape(&entity.name),
            written
                .get(&entity.id)
                .map_or(String::new(), |m| day_of(*m)),
        ));
    }
    list.push_str("</ul>\n");
    let mut foot = format!(
        "<a href=\"{KIND_PATH}{}/\">all {}</a>",
        pages::escape(kind),
        members.len()
    );
    // The org chart is the bots' own way in.
    if kind == "bot" {
        foot.push_str(&format!(" · <a href=\"{}\">org chart</a>", org::PATH));
    }
    shell::boxed_as(
        &format!("kind-{kind}"),
        false,
        &pages::escape(kind),
        &members.len().to_string(),
        &format!("{list}<div class=\"foot\">{foot}</div>"),
    )
}

/// **The box of what is due**, from the due view run as its page runs it. A view
/// that cannot be run puts its words in the box instead.
async fn due_box(state: &AppState, by_id: &HashMap<&EntityId, &Entity>) -> String {
    match pages::answer_named_view(state, "view:due").await {
        Err(why) => shell::boxed_as("due", true, "due", "", &why),
        Ok(answer) => {
            let mut list = String::from("<ul>\n");
            for object in &answer.found {
                list.push_str(&format!(
                    "<li><span class=\"t\"><a href=\"{}\">{}</a></span></li>\n",
                    pages::escape(&tree::canonical_path(&object.entity, by_id)),
                    pages::escape(&object.entity.name),
                ));
            }
            list.push_str("</ul>\n");
            shell::boxed_as(
                "due",
                true,
                "due",
                &answer.found.len().to_string(),
                &if answer.found.is_empty() {
                    "<p>Nothing is due.</p>".to_string()
                } else {
                    list
                },
            )
        }
    }
}

/// **Everything the homepage lists**: the roots, the kind boxes and the due box,
/// or the page that says the store could not be read.
pub async fn body(
    state: &AppState,
    entities: &[Entity],
    sort: &Sort,
) -> Result<String, MemoryError> {
    let written = last_writes(state).await?;
    let by_id: HashMap<&EntityId, &Entity> = entities.iter().map(|e| (&e.id, e)).collect();
    let standing = standing(entities);
    let inside = inside_of(&standing);

    let day_of = day_of_in_the_instance_zone(state).await;
    let mut kinds: BTreeMap<&str, Vec<&Entity>> = BTreeMap::new();
    for entity in &standing {
        kinds
            .entry(entity.kind.as_token())
            .or_default()
            .push(entity);
    }
    let mut boxes: Vec<String> = kinds
        .iter()
        .map(|(kind, members)| kind_box(kind, members, &written, &by_id, &day_of))
        .collect();
    boxes.push(due_box(state, &by_id).await);

    let roots: Vec<&Entity> = standing
        .iter()
        .copied()
        .filter(|e| e.parent.is_none())
        .collect();
    let roots = ordered(roots, &written, sort.by_write());
    let note = format!("top level · {} things · {}", roots.len(), sort_links("/"));
    let table = listing("roots", &roots, &inside, &written, &by_id, &day_of);
    let content = format!(
        "{}\n{}",
        shell::grid(&boxes),
        shell::boxed_as("top", false, "/", &note, &table)
    );
    Ok(content)
}

/// `/ui/kind/<kind>/` — every thing of one kind that stands, in the shell. A kind
/// nothing holds has no page.
pub async fn kind_page(
    State(state): State<AppState>,
    Path(kind): Path<String>,
    Query(sort): Query<Sort>,
) -> Response {
    let entities = match state.memory.list_entities(None).await {
        Ok(entities) => entities,
        Err(err) => return pages::unreadable(err),
    };
    let by_id: HashMap<&EntityId, &Entity> = entities.iter().map(|e| (&e.id, e)).collect();
    let standing = standing(&entities);
    let members: Vec<&Entity> = standing
        .iter()
        .copied()
        .filter(|e| e.kind.as_token() == kind)
        .collect();
    if members.is_empty() {
        return pages::not_found();
    }
    let written = match last_writes(&state).await {
        Ok(written) => written,
        Err(err) => return pages::unreadable(err),
    };
    let inside = inside_of(&standing);
    let day_of = day_of_in_the_instance_zone(&state).await;
    let here = format!("{KIND_PATH}{}/", pages::escape(&kind));
    let members = ordered(members, &written, sort.by_write());
    let note = format!("{} things · {}", members.len(), sort_links(&here));
    let table = listing("things", &members, &inside, &written, &by_id, &day_of);
    pages::html(&shell::page(
        &format!("Every {kind}"),
        "",
        &shell::boxed_as("kind", false, &pages::escape(&kind), &note, &table),
    ))
}
