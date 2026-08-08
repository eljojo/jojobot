//! **The one-time carry** — entities, their prose and their facts move out of
//! the document store and into this one.
//!
//! Not a sync and not a dual-write (rule 145): it happens once, every boot asks
//! whether it already has, and after the first one the question costs a single
//! row read.
//!
//! **It is small because the board is small**, and it deliberately does not
//! rebuild the machinery mail and sessions needed. What that carry earned and
//! this one keeps is the pair that made it trustworthy: every record is read
//! back **through the target's own read path** before the store may be served
//! from, and a refusal refuses the boot rather than serving a board nobody
//! checked.
//!
//! **A fact's owner is re-derived from placement, and only this code can do
//! it.** The old store records it nowhere — a fact belongs to the page its row
//! sits on, and the subject cell beside it may say something else. The scan
//! hands each document's own facts back with that page's entity, so the carry
//! writes the placement into the one column a fact now has (rule 147). A carry
//! that took the subject cell instead would file every disagreeing row under
//! the wrong entity, silently and for good.
//!
//! **An old row has no standing**, so it is written NULL rather than the value
//! its provenance implies (rule 148): nobody declared one, and the column says
//! so rather than inventing a claim somebody has to unpick later.
//!
//! **Nothing here deletes anything.** The old documents stay exactly as they
//! are; what happens to them afterwards is a person's decision (rule 60).

use jojobot_domain::memory::{Fact, Memory, MemoryError, search::DocScan};
use sqlx::{MySql, MySqlPool, Transaction};

/// The record this carry writes, in the table the mail-and-sessions one used.
const CARRIED: &str = "memory";
/// The rows are committed and the read-back has not passed yet.
const WRITTEN: &str = "written";
/// The read-back passed. **Only this state lets the store be served from.**
const VERIFIED: &str = "verified";

/// What the boot does about the carry.
#[derive(Debug)]
pub enum Carried {
    /// It ran, and everything read back as itself.
    Carried(Report),
    /// An earlier boot did it, and verified it.
    AlreadyCarried,
    /// **No old store is wired**, so there is nothing anywhere to miss. The one
    /// outcome that is not a failure and not a carry.
    NothingToCarry(String),
    /// It did not complete. The boot must not serve memory from this store.
    Refused(CarryError),
}

/// Why the carry did not complete.
#[derive(Debug, thiserror::Error)]
pub enum CarryError {
    /// The target already holds records of this kind and no record says this
    /// carry put them there. **Nothing was written.**
    #[error(
        "the store already holds {held} {what} and no record says this carry wrote them — \
         nothing was written, and a person has to look at what is there"
    )]
    Populated {
        /// Which kind was already there.
        what: &'static str,
        /// How many.
        held: usize,
    },
    /// The old store could not be read. Nothing was written.
    #[error("the old store could not be read: {0}")]
    Source(String),
    /// The store refused a write.
    #[error("the store refused the carry: {0}")]
    Target(String),
    /// **The record says the rows are committed and the read-back never
    /// finished.** A person has to look before this store is served from.
    #[error(
        "the carry's record says '{state}', not 'verified' — the rows were committed and the \
         read-back never completed, so the store holds records nobody checked"
    )]
    Halfway {
        /// The token the record wears, quoted rather than interpreted.
        state: String,
    },
    /// A record did not read back as what it was.
    #[error("{what} '{which}' did not read back as it was written: {field} differs")]
    Mismatch {
        /// Which kind of record.
        what: &'static str,
        /// Which one, by handle or address.
        which: String,
        /// The first field that differs.
        field: &'static str,
    },
}

/// What crossed. **Read and verified are counted apart**, because a carry that
/// wrote everything and checked nothing is the state this whole module exists
/// to make impossible to report as success.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Entities read from the old store.
    pub entities: usize,
    /// Facts read from the old store.
    pub facts: usize,
    /// Entities whose prose came across with them.
    pub prose: usize,
    /// Records compared against the target's own read path afterwards.
    pub verified: usize,
}

/// **The verb a boot calls.** It answers the question the boot actually has —
/// may memory be served from this store — and carries only when the answer is
/// "not yet".
pub async fn carry_over(
    from: &dyn Memory,
    to: &super::memory::DoltMemory,
    pool: &MySqlPool,
) -> Carried {
    match recorded(pool).await {
        Err(why) => Carried::Refused(why),
        Ok(Some(state)) if state == VERIFIED => Carried::AlreadyCarried,
        Ok(Some(state)) => Carried::Refused(CarryError::Halfway { state }),
        Ok(None) => match run(from, to, pool).await {
            Ok(report) => Carried::Carried(report),
            Err(CarryError::Source(why)) if why.contains("not configured") => {
                Carried::NothingToCarry(why)
            }
            Err(refused) => Carried::Refused(refused),
        },
    }
}

/// Carry everything across, then prove it.
pub async fn run(
    from: &dyn Memory,
    to: &super::memory::DoltMemory,
    pool: &MySqlPool,
) -> Result<Report, CarryError> {
    // **Read first, refuse before writing anything.** A carry that discovered a
    // populated target halfway would leave a mixture nobody can reason about.
    let scanned = from.scan().await.map_err(source)?;

    let mut tx = pool.begin().await.map_err(target)?;
    must_be_empty(&mut tx, "entity", "entities").await?;
    must_be_empty(&mut tx, "fact", "facts").await?;

    // The record goes in with the rows it is about, so there is no state where
    // the records are committed and nothing says a carry happened.
    sqlx::query("INSERT INTO handover (what, state) VALUES (?, ?)")
        .bind(CARRIED)
        .bind(WRITTEN)
        .execute(&mut *tx)
        .await
        .map_err(target)?;

    let mut report = Report::default();
    for doc in &scanned {
        // A page carrying no entity is somebody's own writing, not a record
        // this store holds: it has no handle to file under, and inventing one
        // would be creating an entity as a side effect of a migration.
        let Some(entity) = &doc.entity else {
            continue;
        };
        sqlx::query(
            "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(entity.id.as_str())
        .bind(entity.kind.as_token())
        .bind(&entity.name)
        .bind(&entity.source)
        .bind(entity.crm.as_deref())
        .bind(entity.parent.as_ref().map(|p| p.as_str()))
        .bind(entity.boot.as_token())
        .bind(&doc.prose)
        .execute(&mut *tx)
        .await
        .map_err(target)?;
        report.entities += 1;
        if !doc.prose.is_empty() {
            report.prose += 1;
        }
        for (ordinal, alias) in entity.aliases.iter().enumerate() {
            sqlx::query("INSERT INTO entity_alias (entity, ordinal, alias) VALUES (?, ?, ?)")
                .bind(entity.id.as_str())
                .bind(ordinal as i64 + 1)
                .bind(alias)
                .execute(&mut *tx)
                .await
                .map_err(target)?;
        }
        for fact in &doc.facts {
            write_fact(&mut tx, doc, fact).await?;
            report.facts += 1;
        }
    }

    tx.commit().await.map_err(target)?;

    // **Read back through the target's own read path**, never the rows just
    // written: a comparison against this module's memory of what it sent would
    // agree with itself whatever the store did with it.
    verify(&mut report, &scanned, to).await?;

    sqlx::query("UPDATE handover SET state = ? WHERE what = ?")
        .bind(VERIFIED)
        .bind(CARRIED)
        .execute(pool)
        .await
        .map_err(target)?;
    Ok(report)
}

/// One fact, filed under **the page it sat on**.
async fn write_fact(
    tx: &mut Transaction<'_, MySql>,
    doc: &DocScan,
    fact: &Fact,
) -> Result<(), CarryError> {
    let home = doc
        .entity
        .as_ref()
        .expect("a doc with facts to carry has an entity")
        .id
        .clone();
    sqlx::query(
        "INSERT INTO fact (entity, id, content, details, provenance, standing, status, date,
                           edge_shape, edge_object, event_kind, derived_from, derived_from_id)
         VALUES (?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(home.as_str())
    .bind(fact.id.as_str())
    .bind(&fact.content)
    .bind(fact.details.as_deref())
    .bind(fact.provenance.as_token())
    .bind(fact.status.as_token())
    .bind(fact.date.to_string())
    .bind(fact.edge.as_ref().map(|e| e.shape.as_token()))
    .bind(fact.edge.as_ref().map(|e| e.object.as_str()))
    .bind(fact.event.as_ref().map(|e| e.kind.as_str()))
    .bind(fact.derived_from.as_ref().map(|d| d.home.as_str()))
    .bind(fact.derived_from.as_ref().map(|d| d.local.as_str()))
    .execute(&mut **tx)
    .await
    .map_err(target)?;

    let Some(event) = &fact.event else {
        return Ok(());
    };
    for (key, value) in &event.metadata {
        sqlx::query(
            "INSERT INTO fact_event_metadata (fact_home, fact_id, `key`, value)
             VALUES (?, ?, ?, ?)",
        )
        .bind(home.as_str())
        .bind(fact.id.as_str())
        .bind(key)
        .bind(value)
        .execute(&mut **tx)
        .await
        .map_err(target)?;
    }
    for (ordinal, object) in event.refs.iter().enumerate() {
        sqlx::query(
            "INSERT INTO fact_event_ref (fact_home, fact_id, ordinal, entity) VALUES (?, ?, ?, ?)",
        )
        .bind(home.as_str())
        .bind(fact.id.as_str())
        .bind(ordinal as i64 + 1)
        .bind(object.as_str())
        .execute(&mut **tx)
        .await
        .map_err(target)?;
    }
    Ok(())
}

/// Compare every carried record against what the new store hands back.
///
/// **Field by field, through the port.** Counting what was written proves that
/// writes happened; it does not prove that what landed is what was there.
async fn verify(
    report: &mut Report,
    scanned: &[DocScan],
    to: &super::memory::DoltMemory,
) -> Result<(), CarryError> {
    let landed = to.list_entities(None).await.map_err(unread_back)?;
    for doc in scanned {
        let Some(was) = &doc.entity else {
            continue;
        };
        let Some(now) = landed.iter().find(|e| e.id == was.id) else {
            return Err(CarryError::Mismatch {
                what: "entity",
                which: was.id.to_string(),
                field: "the entity itself is not there",
            });
        };
        for (field, differs) in [
            ("name", now.name != was.name),
            ("aliases", now.aliases != was.aliases),
            ("source", now.source != was.source),
            ("crm", now.crm != was.crm),
            ("parent", now.parent != was.parent),
            ("boot", now.boot != was.boot),
        ] {
            if differs {
                return Err(CarryError::Mismatch {
                    what: "entity",
                    which: was.id.to_string(),
                    field,
                });
            }
        }
        report.verified += 1;

        // **Every fact the page held, read back by the handle it now hangs
        // off.** The comparison is on content and provenance rather than on
        // the whole record, because the two the carry deliberately changes —
        // the owner it derived and the standing it left undeclared — are what
        // it is not entitled to find unchanged.
        let recalled = to.recall(&was.id).await.map_err(unread_back)?;
        for fact in &doc.facts {
            let Some(now) = recalled.iter().find(|f| f.id == fact.id) else {
                return Err(CarryError::Mismatch {
                    what: "fact",
                    which: fact.address().to_string(),
                    field: "the row itself is not there",
                });
            };
            for (field, differs) in [
                ("content", now.content != fact.content),
                ("details", now.details != fact.details),
                ("provenance", now.provenance != fact.provenance),
                ("status", now.status != fact.status),
                ("date", now.date != fact.date),
                ("edge", now.edge != fact.edge),
                ("event", now.event != fact.event),
                ("derived_from", now.derived_from != fact.derived_from),
                ("owner", now.home != was.id),
            ] {
                if differs {
                    return Err(CarryError::Mismatch {
                        what: "fact",
                        which: fact.address().to_string(),
                        field,
                    });
                }
            }
            report.verified += 1;
        }
    }
    Ok(())
}

/// Refuse if the target already holds anything of this kind.
async fn must_be_empty(
    tx: &mut Transaction<'_, MySql>,
    table: &str,
    what: &'static str,
) -> Result<(), CarryError> {
    // The table name is this module's own literal, never a caller's.
    let held: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM `{table}`"))
        .fetch_one(&mut **tx)
        .await
        .map_err(target)?;
    if held > 0 {
        return Err(CarryError::Populated {
            what,
            held: held as usize,
        });
    }
    Ok(())
}

/// The record's state, read the way an operator would.
async fn recorded(pool: &MySqlPool) -> Result<Option<String>, CarryError> {
    sqlx::query_scalar::<_, String>("SELECT state FROM handover WHERE what = ?")
        .bind(CARRIED)
        .fetch_optional(pool)
        .await
        .map_err(target)
}

fn source(e: MemoryError) -> CarryError {
    CarryError::Source(e.to_string())
}

fn target(e: sqlx::Error) -> CarryError {
    CarryError::Target(e.to_string())
}

/// **A read-back that could not run is the TARGET's failure, not the source's.**
/// It happens after the commit, so the rows are in and nobody has checked them:
/// reporting it as a source failure would say nothing was written.
fn unread_back(e: MemoryError) -> CarryError {
    CarryError::Mismatch {
        what: "the read-back",
        which: e.to_string(),
        field: "it could not be taken at all",
    }
}
