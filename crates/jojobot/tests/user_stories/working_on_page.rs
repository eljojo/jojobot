//! "If I visit the project's page, I should see the bot is working on it
//! through the graph connections."
//!
//! A piece of work's page names each bot whose live `working_on` points at it or
//! at a piece of work under it, and below that lists the past runs that declared
//! it. A project's page shows the same, rolled up from everything under it. A
//! bot whose writing session wrapped or was abandoned is no longer on it now and
//! is in the past list. A write alone never counts: only a declared `working_on`
//! does.
//!
//! The roster names are the cast: a project, two pieces of work under it, and
//! bots Otto and the assistant the software ships.

use serde_json::json;

use super::dsl::{Session, Story};

/// How the session that said what Otto is working on has ended, if it has.
#[derive(Clone, Copy)]
enum Writer {
    Live,
    Wrapped,
    /// Stopped without a wrap, and swept by a later boot.
    Abandoned,
}

/// A project with two pieces of work under it, and Otto declaring `work:sigma`.
/// Returns the story and Otto's run name.
async fn otto_on_sigma(writer: Writer) -> (Story, String) {
    let story = match writer {
        Writer::Abandoned => Story::begin_pretending_it_is("bot:otto", "2026-06-01").await,
        _ => Story::begin("bot:otto").await,
    };
    let otto = story.session().await;
    otto.add("project:atlas", "Atlas").await;
    otto.add_under("project:atlas", "work:sigma", "Sigma").await;
    otto.add_under("project:atlas", "work:phi", "Phi").await;
    otto.call(
        "journal",
        json!({"entry": "picked up sigma", "working_on": "work:sigma"}),
    )
    .await;
    let run = otto.run().to_string();
    match writer {
        Writer::Live => (story, run),
        Writer::Wrapped => {
            otto.call("wrap_session", json!({"story": "done for now"}))
                .await;
            (story, run)
        }
        Writer::Abandoned => {
            let july = story.restarted_pretending_it_is("2026-07-20").await;
            let _ = july.full_boot().await;
            (july, run)
        }
    }
}

/// **A bot working on a piece of work is on its page, and on its project's.**
/// The sibling that nobody declared is the paired negative.
#[tokio::test]
async fn a_live_bot_is_on_the_page_of_its_work_and_of_the_project_above_it() {
    let (story, _) = otto_on_sigma(Writer::Live).await;

    let work = story.page("project:atlas/work:sigma").await;
    work.section("working-on-now").says("bot:otto");
    let project = story.page("project:atlas").await;
    project
        .section("working-on-now")
        .says("bot:otto")
        .says("work:sigma");
    story
        .page("project:atlas/work:phi")
        .await
        .never_says("id=\"working-on-now\"")
        .says("id=\"working-on-none\"");
    story.finish().await;
}

/// **After its writer wraps, the bot is in the past list and not on the page
/// as working on it now**, on the work and on the project.
#[tokio::test]
async fn a_bot_whose_writer_wrapped_moves_to_the_past_list() {
    let (story, _) = otto_on_sigma(Writer::Wrapped).await;
    for handle in ["project:atlas/work:sigma", "project:atlas"] {
        let page = story.page(handle).await;
        page.never_says("id=\"working-on-now\"")
            .says("id=\"working-on-none\"");
        page.section("working-on-past").says("bot:otto");
    }
    story.finish().await;
}

/// **An abandoned writer likewise**, and the past list is dated.
#[tokio::test]
async fn a_bot_whose_writer_was_abandoned_moves_to_the_past_list_with_its_day() {
    let (story, _) = otto_on_sigma(Writer::Abandoned).await;
    for handle in ["project:atlas/work:sigma", "project:atlas"] {
        let page = story.page(handle).await;
        page.never_says("id=\"working-on-now\"");
        page.section("working-on-past")
            .says("bot:otto")
            .says("2026-06-01");
    }
    story.finish().await;
}

/// **Each past run links to its session**, and the bot's page carries the run's
/// row under that anchor, so the link lands on it.
#[tokio::test]
async fn a_past_run_links_to_its_session_on_the_bots_page() {
    let (story, run) = otto_on_sigma(Writer::Wrapped).await;
    let past = story
        .page("project:atlas/work:sigma")
        .await
        .section("working-on-past");
    past.says(&format!("#session-{run}"));
    story
        .page("bot:otto")
        .await
        .says(&format!("id=\"session-{run}\""));
    story.finish().await;
}

/// **A bot that only wrote to the work is not shown as on it**: a write alone
/// never counts, so only the bot that declared appears.
#[tokio::test]
async fn a_bot_that_only_wrote_to_the_work_is_absent() {
    let (story, _) = otto_on_sigma(Writer::Live).await;
    let writer = story.as_bot("bot:assistant").await;
    writer
        .call(
            "capture",
            json!({"subject": "work:sigma", "content": "looked at it",
                   "provenance": "testimony", "sets": {"status": "now"}}),
        )
        .await;

    let page = story.page("project:atlas/work:sigma").await;
    let now: Session = writer; // keep the session alive across the read
    page.section("working-on-now")
        .says("bot:otto")
        .never_says("bot:assistant");
    let _ = now;
    story.finish().await;
}
