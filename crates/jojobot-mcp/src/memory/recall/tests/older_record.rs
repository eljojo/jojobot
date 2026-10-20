//! The walk over a dependency an older record wrote as its own field, kept apart
//! so the file of recall cases holds its size.

use super::*;

/// **An older record that kept a dependency as its own field is walked by what
/// the thing holds now.** On a store from before own fields stayed on their
/// record, such a key reaches its thing, so a later write that shortens it
/// removes a dependency from the walk as it does from the ready view.
#[tokio::test]
async fn a_walk_leaves_out_a_dependency_an_older_record_wrote_as_its_own_field() {
    let jojobot = crate::harness::handler_shipped_reaching();
    work_with(&jojobot, "work:lambda", &[("status", "next")]).await;
    work_with(&jojobot, "work:gamma", &[("status", "next")]).await;
    work_with(&jojobot, "work:iota", &[("status", "next")]).await;
    for (depends_on, content) in [
        ("work:lambda, work:gamma", "needs both"),
        ("work:gamma", "only gamma is needed now"),
    ] {
        capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [("depends_on".to_string(), depends_on.to_string())]
                        .into_iter()
                        .collect(),
                ),
                sid: Some(writing_as(&jojobot)),
                ..capture_args("work:iota", content)
            },
        )
        .await;
    }
    walks_agree_with_ready(&jojobot).await;
}
