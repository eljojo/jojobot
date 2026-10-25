use jojobot_adapters::dolt::memory::{LinkRuleDiagnostics, LinkRuleSummary};

pub(super) fn log(report: &LinkRuleDiagnostics) {
    let summary = &report.summary;
    tracing::info!(
        target: "jojobot::wiring::field_links",
        values_read = summary.values_read,
        bare_ids = summary.bare_ids,
        mixed_lists = summary.mixed_lists,
        handles = summary.handles,
        unresolved = summary.unresolved,
        ambiguous = summary.ambiguous,
        r1_owned_now = summary.owned_now,
        r1_addressable = report.owned_now_fields.len(),
        r1_unaddressable = report.owned_now_unaddressable,
        r1_unaddressable_holders = report.owned_now_unaddressable_holders,
        supplied_only_current = report.supplied_only_current,
        supplied_only_historical = report.supplied_only_historical,
        links_dropped = summary.links_dropped,
        links_added = summary.links_added,
        inflated_by = LinkRuleSummary::INFLATED_BY,
        data = "before the startup repairs of stored rows, which run after this step",
        "link rule (log-only): what the rule would store and link; nothing was changed"
    );
    for location in &report.owned_now_fields {
        tracing::info!(
            target: "jojobot::wiring::field_links",
            diagnostic = "link_rule_r1_owned_now",
            entity = %location.entity,
            key = %location.key,
            "link rule (log-only): R1 field needs operator repair"
        );
    }
}
