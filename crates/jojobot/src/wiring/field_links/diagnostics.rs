use jojobot_adapters::dolt::memory::{DoltMemory, LinkRuleDiagnostics, LinkRuleSummary};
use jojobot_domain::memory::LinkRule;

pub(super) async fn read(linking: &DoltMemory, rule: LinkRule) {
    match linking.link_rule_diagnostics().await {
        Ok(report) => log(&report, rule),
        Err(e) => {
            tracing::warn!(error = %e, mode = ?rule, "link rule: the summary could not be read")
        }
    }
}

fn log(report: &LinkRuleDiagnostics, rule: LinkRule) {
    let summary = &report.summary;
    let (mode, message, stage) = match rule {
        LinkRule::On => (
            "on",
            "link rule (on): conversion summary",
            "after bare line repairs and before field-link marking",
        ),
        LinkRule::LogOnly => (
            "log-only",
            "link rule (log-only): what the rule would store and link; nothing was changed",
            "before the startup repairs of stored rows, which run after this step",
        ),
    };
    tracing::info!(
        target: "jojobot::wiring::field_links",
        values_read = summary.values_read,
        bare_ids = summary.bare_ids,
        mixed_lists = summary.mixed_lists,
        handles = summary.handles,
        unresolved = summary.unresolved,
        ambiguous = summary.ambiguous,
        r1_owned_now = summary.owned_now,
        r1_historical = report.owned_historical,
        r1_addressable = report.owned_now_fields.len() + report.owned_now_fields_left_out,
        r1_locations_left_out = report.owned_now_fields_left_out,
        r1_unaddressable = report.owned_now_unaddressable,
        r1_unaddressable_holders = report.owned_now_unaddressable_holders,
        supplied_only_current = report.supplied_only_current,
        supplied_only_historical = report.supplied_only_historical,
        links_dropped = summary.links_dropped,
        links_added = summary.links_added,
        inflated_by = LinkRuleSummary::INFLATED_BY,
        data = stage,
        "{message}"
    );
    for location in &report.owned_now_fields {
        tracing::info!(
            target: "jojobot::wiring::field_links",
            diagnostic = "link_rule_r1_owned_now",
            entity = %location.entity,
            key = %location.key,
            "link rule ({mode}): R1 field needs operator repair"
        );
    }
}
