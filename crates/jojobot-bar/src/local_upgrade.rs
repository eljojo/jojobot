//! The deployed-store cases that require CI's fixture.
//! This applies only to narrow; the boundary and report bars keep every case.

pub const CASES: &[&str] = &[
    "a_hand_made_instance_record_keeps_its_operator_and_zone_across_a_boot",
    "a_boot_over_a_run_with_a_long_history_reaches_its_serving_line",
    "a_lost_link_table_is_rebuilt_at_the_next_boot_on_the_last_deployed_store",
    "a_pointer_at_a_record_the_build_supplies_boots_quiet_and_reads_back",
    "a_reference_field_naming_a_supplied_record_boots_quiet_and_reads_back",
    "a_held_value_the_oracle_cannot_join_fails_the_gate",
    "a_store_holding_a_row_at_a_handle_the_build_supplies_boots_and_the_row_answers",
    "a_stored_cadence_days_rhythm_checks_in_on_the_head_as_it_did_before",
    "a_stored_cadence_days_rhythm_reads_the_same_on_the_head",
    "a_stored_mark_inside_code_reads_as_the_handle_on_the_current_binary",
    "a_thread_a_deployed_build_left_over_the_default_ceiling_reads_and_edits_as_before",
    "a_view_stored_with_a_filter_record_answers_as_before_on_the_current_binary",
    "every_recorded_held_value_joins_to_an_entity",
    "everything_naming_a_handle_the_build_stopped_supplying_follows_the_row_the_boot_gave_it",
    "field_link_holds_the_links_the_writes_imply_and_nothing_else",
    "kitchen_sink::the_kitchen_sink_boots_once_and_reads_every_tolerated_shape",
    "kitchen_sink::link_marking_changes_no_row_on_the_second_boot",
    "kitchen_sink::old_merge_repair_changes_no_row_on_the_second_boot",
    "rows_classed_own_still_reach_their_things_on_the_last_deployed_store",
    "runs_in_moment_order::the_runs_of_the_last_deployed_store_read_in_moment_order_on_the_head",
    "the_boots_over_a_store_the_deployed_build_filled_log_no_warning_and_no_error",
    "the_current_binary_boots_on_the_last_deployed_store",
    "the_last_deployed_store_recorded_six_hours_earlier_reads_back",
    "the_last_deployed_store_recorded_three_days_earlier_reads_back",
    "the_scan_for_oversized_records_finds_one_stored_over_the_ceiling",
    "what_the_operator_wrote_on_a_handle_the_build_stopped_supplying_reads_back_as_a_thing",
];

pub fn requires_fixture(name: &str) -> bool {
    name.starts_with("deaths::") || CASES.contains(&name)
}
