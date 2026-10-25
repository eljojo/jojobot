//! The field-link step of the boot: the pass the link rule selects, then, while the
//! rule is log-only, the read-only summary of what it would do.

use jojobot_adapters::dolt::Migrated;
use jojobot_adapters::dolt::memory::DoltMemory;

mod diagnostics;

/// **Run the field-link boot step the link rule selects, and log it.** Log-only
/// runs the pass as it was before the rule and then logs the counts of what the
/// rule would do; on runs the marking pass. Nothing here writes in log-only
/// beyond what the pass before the rule wrote.
pub async fn boot_field_links(linking: &DoltMemory) {
    // **Under `on`, the repair of bare line values comes first**, with the rule
    // before this one: it selects the bare-name link rows the marked reconcile
    // removes, and it leaves a bare id for the marking to mark. The repairs the
    // boot runs after this step find it settled. **A repair that could not run
    // leaves the marking for the next boot**, because marking would delete the rows
    // the repair is still owed.
    if jojobot_domain::memory::link_rule() == jojobot_domain::memory::LinkRule::On {
        match linking.repair_bare_line_values_ahead().await {
            Migrated::Failed(why) | Migrated::Skipped(why) => {
                tracing::error!(
                    pass = "bare line values",
                    error = %why,
                    "BARE LINE VALUES NOT REPAIRED — the marking of field links is left for the \
                     next boot, so the rows the repair still owes are not removed under it."
                );
                return;
            }
            Migrated::Ran(rows) => {
                tracing::info!(
                    pass = "bare line values",
                    rows,
                    "store: repair pass finished"
                )
            }
            Migrated::Settled => {
                tracing::info!(pass = "bare line values", "store: repair pass already done")
            }
        }
    }
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
                match jojobot_domain::memory::link_rule() {
                    jojobot_domain::memory::LinkRule::On => {
                        tracing::info!("store: field value links marked: already done")
                    }
                    jojobot_domain::memory::LinkRule::LogOnly => {
                        tracing::info!("store: field value lowering: already done")
                    }
                }
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
        match linking.link_rule_diagnostics().await {
            Ok(report) => diagnostics::log(&report),
            Err(e) => {
                tracing::warn!(error = %e, "link rule (log-only): the summary could not be read")
            }
        }
    }
}
