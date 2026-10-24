//! The field-link step of the boot: the pass the link rule selects, then, while the
//! rule is log-only, the read-only summary of what it would do.

use jojobot_adapters::dolt::memory::DoltMemory;

/// **Run the field-link boot step the link rule selects, and log it.** Log-only
/// runs the pass as it was before the rule and then logs the counts of what the
/// rule would do; on runs the marking pass. Nothing here writes in log-only
/// beyond what the pass before the rule wrote.
pub async fn boot_field_links(linking: &DoltMemory) {
    let field_links = match jojobot_domain::memory::link_rule() {
        jojobot_domain::memory::LinkRule::On => linking.migrate_field_links_marked().await,
        jojobot_domain::memory::LinkRule::LogOnly => linking.migrate_field_links().await,
    };
    match field_links {
        Ok(report) => {
            if report.disagreed {
                tracing::warn!(
                    linked = report.linked,
                    unlinked = report.unlinked,
                    "LINK TABLE DISAGREED WITH THE FIELD WRITES — it was rebuilt from them. The \
                     table is derived, so nothing was lost; a disagreement means a build or a \
                     failure left it behind."
                );
            }
            if report.settled {
                tracing::info!("store: field value lowering: already done");
            }
            if report.lowered > 0 || report.linked > 0 || report.unlinked > 0 {
                tracing::info!(
                    lowered = report.lowered,
                    linked = report.linked,
                    unlinked = report.unlinked,
                    "store: lowered plain handle text onto the ids it names and updated the link \
                     table"
                );
            }
            for line in &report.left_as_text {
                tracing::warn!(
                    value = %line,
                    "FIELD VALUE LEFT AS TEXT — the handle in it does not name exactly one thing, \
                     so it was not lowered and links nothing. A person has to say which thing it \
                     meant."
                );
            }
        }
        Err(e) => tracing::error!(
            error = %e,
            "FIELD LINKS NOT BUILT — who points at a thing is answered from the link table, so \
             some links may not be found. Nothing was lost; a restart repeats the pass."
        ),
    }

    // **What the link rule of rule 646 would do, counted and logged while it is
    // log-only.** Read-only: it changes no byte, link row or pass marker. The
    // counts name no value.
    if jojobot_domain::memory::link_rule() == jojobot_domain::memory::LinkRule::LogOnly {
        match linking.link_rule_summary().await {
            Ok(summary) => tracing::info!(
                values_read = summary.values_read,
                bare_ids = summary.bare_ids,
                mixed_lists = summary.mixed_lists,
                handles = summary.handles,
                unresolved = summary.unresolved,
                ambiguous = summary.ambiguous,
                r1_owned_now = summary.owned_now,
                links_dropped = summary.links_dropped,
                links_added = summary.links_added,
                inflated_by = jojobot_adapters::dolt::memory::LinkRuleSummary::INFLATED_BY,
                data = "before the startup repairs of stored rows, which run after this step",
                "link rule (log-only): what the rule would store and link; nothing was changed"
            ),
            Err(e) => {
                tracing::warn!(error = %e, "link rule (log-only): the summary could not be read")
            }
        }
    }
}
