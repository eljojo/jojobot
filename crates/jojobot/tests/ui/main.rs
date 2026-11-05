//! One browser UI test binary shares its issuer and browser helpers across the area.

#[path = "../support/mod.rs"]
mod support;

#[path = "../ui.rs"]
mod ui;
mod ui_client_id_is_not_the_audience;
mod ui_fenced_details;
mod ui_home_written;
mod ui_instance_unread;
mod ui_operator_mail;
mod ui_past_runs_date;
mod ui_read_path;
mod ui_read_path_typed;
mod ui_search_query;
mod ui_view_query;
