//! **What a plan's period holds, read when the plan is read.**
//!
//! A list that carries `covers` is a plan for one calendar day or one ISO week.
//! Its read adds what falls due in that period, what fell due before it and is
//! still owed, and which of that work can be started. Nothing here is stored on
//! the plan: the questions are the ones the `due` and `ready` views ask, through
//! the same passes ([`view_run::keep_owed`], [`ready::verdicts_of`]), so a plan
//! cannot say a different thing from the views.

use jiff::civil::Date;
use jojobot_domain::attention;
use jojobot_domain::memory::graph;

use super::view_run;
use super::*;

/// **The most items one section of a plan names**, whole items only. What is left
/// out is counted, and the due view reads all of it.
const SECTION_ROOM: usize = 25;

impl Jojobot {
    /// **The plan block for a list that covers `from` to `to`**, read now.
    /// Nothing when the store cannot answer: a plan that cannot be read is left
    /// off the list's read rather than served wrong.
    pub(crate) async fn plan_of_period(
        &self,
        covers: &str,
        from: Date,
        to: Date,
    ) -> Option<serde_json::Value> {
        let question = view_run::Question::new(
            graph::Selection {
                owed: true,
                ..graph::Selection::default()
            },
            graph::Include {
                facts: false,
                prose: false,
                stood_for: false,
                status: None,
            },
            None,
            None,
        );
        let mut found = view_run::select(self.memory.as_ref(), &[], &question.query, None)
            .await
            .ok()?
            .objects;
        let instance = self.instance_fields().await;
        let carriers = self.carriers();
        // Owed by the last day of the period, measured from the first: a thing
        // that fell due before the period began is that many days late at its
        // start.
        view_run::keep_owed(&mut found, &carriers, &instance, to, from);

        let mut due = Vec::new();
        let mut overdue = Vec::new();
        for object in &found {
            let mut item = serde_json::json!({
                "handle": object.entity.id.to_string(),
                "name": object.entity.name,
            });
            match view_run::owed_by(&carriers, object, &instance) {
                attention::Due::On(day) if day >= from => {
                    item["due_on"] = day.to_string().into();
                    due.push(item);
                }
                attention::Due::On(day) => {
                    item["due_on"] = day.to_string().into();
                    item["overdue_by_days"] = i64::from(from.since(day).ok()?.get_days()).into();
                    overdue.push(item);
                }
                // Late and unable to say by how much: it is overdue, loudly.
                _ => overdue.push(item),
            }
        }

        let asked: Vec<ready::Asked<'_>> = found
            .iter()
            .filter(|object| matches!(object.entity.kind, EntityKind::WORK | EntityKind::PROJECT))
            .map(|object| ready::Asked {
                kind: object.entity.kind.as_token(),
                fields: &object.fields,
                archived: object.entity.archived.is_some(),
            })
            .collect();
        let verdicts = ready::verdicts_of(self.memory.as_ref(), &asked)
            .await
            .ok()?;
        let ready_items: Vec<serde_json::Value> = found
            .iter()
            .filter(|object| matches!(object.entity.kind, EntityKind::WORK | EntityKind::PROJECT))
            .zip(&verdicts)
            .filter(|(_, verdict)| verdict.ready)
            .map(|(object, _)| {
                serde_json::json!({
                    "handle": object.entity.id.to_string(),
                    "name": object.entity.name,
                })
            })
            .collect();

        let mut plan = serde_json::json!({
            "covers": covers,
            "from": from.to_string(),
            "to": to.to_string(),
        });
        for (section, items) in [("due", due), ("overdue", overdue), ("ready", ready_items)] {
            let left_out = items.len().saturating_sub(SECTION_ROOM);
            plan[section] = items
                .into_iter()
                .take(SECTION_ROOM)
                .collect::<Vec<_>>()
                .into();
            if left_out > 0 {
                plan[format!("{section}_left_out")] = serde_json::json!({
                    "count": left_out,
                    "how_to_read": "recall the due view, or the ready view, to read all of it",
                });
            }
        }
        Some(plan)
    }
}
