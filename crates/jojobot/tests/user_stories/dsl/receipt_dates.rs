use super::*;

impl Session {
    /// Something the person said, with no date on it — and **the day jojobot
    /// stamped it with**, which is the run's own day rather than the server's.
    pub async fn undated_fact(&self, subject: &str, content: &str) -> String {
        let body = self
            .write(
                &format!("an undated fact about {subject}"),
                "capture",
                json!({"subject": subject, "content": content, "provenance": "testimony"}),
            )
            .await;
        assert!(
            body.get("recorded_at").is_none(),
            "the input is not echoed: {body}"
        );
        body["derived"]["recorded_at"]
            .as_str()
            .unwrap_or_else(|| panic!("a capture is stamped with a day: {body}"))
            .to_string()
    }
}
