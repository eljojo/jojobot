//! A text that is blank is refused.

use super::*;

/// **A text with nothing to put is refused by name.**
#[tokio::test]
async fn an_empty_or_blank_text_or_a_lone_carriage_return_is_refused() {
    let jojobot = handler();
    ensure(&jojobot, "document:alpha").await;
    for text in ["", "  \n \n", "# One\rtwo"] {
        let refused = put(&jojobot, put_args("document:alpha", text).await).await;
        blocked(&refused);
    }
    assert!(records(&jojobot, "document:alpha").await.is_empty());
}
