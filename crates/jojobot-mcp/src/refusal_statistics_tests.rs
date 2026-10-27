//! Refusal codes travel through the served router and its real counting exit.
use crate::{Jojobot, harness::*};
use jojobot_domain::usage::{call_fields, run_verbs, weeks};
use rmcp::{RoleServer, model::*, service::serve_directly, transport::OneshotTransport};
use serde_json::{Value, json};

async fn served(server: &Jojobot, verb: &str, mut args: Value) -> Value {
    args["sid"] = TEST_SID.into();
    let request = serde_json::from_value(json!({
        "jsonrpc":"2.0", "id":1, "method":"tools/call",
        "params":{"name":verb, "arguments":args}
    }))
    .unwrap();
    let (transport, mut replies) = OneshotTransport::<RoleServer>::new(request);
    let running = serve_directly(server.clone(), transport, Some(ClientInfo::default()));
    let response = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let reply = replies.recv().await.expect("the router answers");
            let value = serde_json::to_value(reply).unwrap();
            if value.get("result").is_some() || value.get("error").is_some() {
                break value;
            }
        }
    })
    .await
    .expect("one in-memory call finishes");
    running.cancel().await.unwrap();
    response
}

async fn call(server: &Jojobot, verb: &str, args: Value) -> Value {
    let response = served(server, verb, args).await;
    if let Some(result) = response.get("result") {
        if let Some(body) = result.get("structuredContent") {
            return body.clone();
        }
        return serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    }
    response["error"]["data"].clone()
}

async fn counted(server: &Jojobot, verb: &str, code: &str) {
    let counts = server.sessions.call_counts().await.unwrap();
    let today = counts[0].key.day;
    let week = weeks(
        &[],
        &counts,
        &[],
        &|at| at.to_zoned(jiff::tz::TimeZone::UTC).date(),
        today,
    );
    let fields = &week[0].projected().fields;
    let runs = server.sessions.run_calls(None).await.unwrap();
    let run_fields: std::collections::BTreeMap<_, _> = call_fields(&run_verbs(&runs, TEST_RUN))
        .into_iter()
        .collect();
    assert_eq!(
        run_fields
            .get(&format!("refused_{verb}_code_{code}"))
            .map(String::as_str),
        Some("1"),
        "{run_fields:?}"
    );
    assert_eq!(
        fields
            .get(&format!("refused_{verb}_code_{code}"))
            .map(String::as_str),
        Some("1"),
        "{fields:?}"
    );
}

#[tokio::test]
async fn refusal_statistics_invalid_fact_is_served_and_counted() {
    let server = handler();
    call(
        &server,
        "add_entity",
        json!({"kind":"list","handle":"errands","name":"Errands","source":"test"}),
    )
    .await;
    let body = call(
        &server,
        "capture",
        json!({"subject":"list:errands","content":""}),
    )
    .await;
    assert_eq!(body["refusal_code"], "InvalidFact", "{body}");
    counted(&server, "capture", "InvalidFact").await;
}

#[tokio::test]
async fn refusal_statistics_unknown_argument_is_served_and_counted() {
    let server = handler();
    let body = call(
        &server,
        "recall",
        json!({"subject":"bot:otto","not_an_argument":true}),
    )
    .await;
    assert_eq!(body["refusal_code"], "UnknownArgument", "{body}");
    counted(&server, "recall", "UnknownArgument").await;
}

#[tokio::test]
async fn refusal_statistics_near_name_is_served_and_counted() {
    let server = handler();
    call(
        &server,
        "add_entity",
        json!({"kind":"person","handle":"milhouse","name":"Milhouse","source":"test"}),
    )
    .await;
    let body = call(
        &server,
        "add_entity",
        json!({"kind":"person","handle":"milhouse-2","name":"Milhouse","source":"test"}),
    )
    .await;
    assert_eq!(body["refusal_code"], "NearName", "{body}");
    counted(&server, "add_entity", "NearName").await;
}

#[tokio::test]
async fn refusal_statistics_unlisted_code_never_becomes_a_count_key() {
    let server = handler();
    let result = Ok(crate::answer::json_result(
        &json!({"status":"blocked","fix_by":"change","refusal_code":"caller-chose-this"}),
    )
    .unwrap()
    .into());
    server
        .count_call(
            "capture".into(),
            server.caller(Some(TEST_SID)).unwrap(),
            None,
            &result,
            Default::default(),
        )
        .await;
    counted(&server, "capture", "unlisted").await;
}

#[test]
fn refusal_statistics_every_lane_sample_has_a_code() {
    let mut answers = Vec::new();
    for (_, error, _) in crate::memory::declined::tests::memory_refusal_rows() {
        answers.push(crate::memory::declined::memory_declined("capture", error));
    }
    for (_, error, _) in crate::mailboxes::declined::tests::mailbox_refusal_rows() {
        answers.push(crate::mailboxes::declined::mailbox_declined(error));
    }
    for (_, error, _) in crate::session::declined::tests::session_refusal_rows() {
        answers.push(crate::session::declined::session_declined(error, TEST_SID));
    }
    assert!(
        context_refusal_codes_are_declared(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
        ) > 0
    );
    for answer in answers {
        let body = match answer {
            Ok(answer) => json_of(&answer),
            Err(error) => error.data.unwrap_or_default(),
        };
        assert!(
            body["refusal_code"]
                .as_str()
                .is_some_and(crate::answer::listed_refusal_code),
            "missing/unlisted code: {body}"
        );
    }
}

fn context_refusal_codes_are_declared(root: &std::path::Path) -> usize {
    let mut checked = 0;
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            checked += context_refusal_codes_are_declared(&path);
            continue;
        }
        if path.extension().is_none_or(|ext| ext != "rs")
            || path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("tests.rs")
        {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let production = text.split("mod tests {").next().unwrap();
        for (at, _) in production.match_indices("\"status\": \"blocked\"") {
            let body = production[at..].split("});").next().unwrap();
            assert!(
                body.contains("\"refusal_code\":"),
                "blocked body has no code: {}",
                path.display()
            );
            checked += 1;
        }
        for rest in production.split("\"refusal_code\": \"").skip(1) {
            let code = rest.split('"').next().unwrap();
            assert!(
                crate::answer::listed_refusal_code(code),
                "unlisted code {code}: {}",
                path.display()
            );
        }
    }
    checked
}

#[tokio::test]
async fn refusal_statistics_protocol_errors_also_have_a_bounded_code() {
    let server = handler();
    let body = call(&server, "not_a_verb", json!({})).await;
    assert_eq!(body["refusal_code"], "ProtocolError", "{body}");
    counted(&server, "unserved", "ProtocolError").await;
}

#[tokio::test]
async fn refusal_statistics_sdk_type_errors_are_served_counted_and_observed() {
    use crate::observe::{CallEnd, CallObserver};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Observed(Mutex<Vec<(String, CallEnd, String)>>);
    impl CallObserver for Observed {
        fn call_answered(&self, verb: &str, end: CallEnd, code: &str, _: std::time::Duration) {
            self.0.lock().unwrap().push((verb.into(), end, code.into()));
        }
    }
    let observed = Arc::new(Observed::default());
    let server = handler().with_call_observer(observed.clone());
    crate::memory::testing::ensure(&server, "person:alpha").await;
    call(
        &server,
        "capture",
        json!({"subject":"person:alpha", "content":"Reads the map"}),
    )
    .await;
    let args = json!({"subject":"person:alpha", "content":17, "sid":TEST_SID});
    let message = serde_json::from_value::<crate::memory::capture::CaptureArgs>(args.clone())
        .unwrap_err()
        .to_string();
    let response = served(&server, "capture", args).await;
    let result = &response["result"];
    let events = observed.0.lock().unwrap().clone();
    assert_eq!(result["isError"], true, "{result}");
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains(&message)
    );
    assert_eq!(
        result["structuredContent"]["refusal_code"], "InvalidArguments",
        "{result}; observer={events:?}"
    );
    counted(&server, "capture", "InvalidArguments").await;
    assert_eq!(
        events,
        [
            ("capture".into(), CallEnd::Answered, String::new()),
            (
                "capture".into(),
                CallEnd::Refused,
                "InvalidArguments".into()
            ),
        ]
    );
}
