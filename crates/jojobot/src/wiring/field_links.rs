//! The field-link step of boot: mark every stored field value's links and bring
//! the link table into agreement with the marks.

use jojobot_adapters::dolt::memory::DoltMemory;

pub async fn boot_field_links(linking: &DoltMemory) {
    match linking.migrate_field_links_marked().await {
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
                tracing::info!("store: field value links marked: already done");
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
}
