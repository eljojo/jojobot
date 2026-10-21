//! **The verbs the running server publishes**, asked the way a client asks. A
//! page that names a verb is held to this list and not to a spelling the test
//! carries.

use std::net::SocketAddr;

use rmcp::ServiceExt;
use rmcp::model::{ClientCapabilities, ClientInfo, Implementation};

/// The name of every tool the server at `addr` lists.
pub async fn names(addr: SocketAddr) -> Vec<String> {
    let transport =
        rmcp::transport::StreamableHttpClientTransport::from_uri(format!("http://{addr}/mcp"));
    let client = ClientInfo::new(
        ClientCapabilities::default(),
        Implementation::new("ui-test", "0.0.1"),
    )
    .serve(transport)
    .await
    .expect("the tool list's client connects");
    let tools = client
        .list_all_tools()
        .await
        .expect("the server lists its tools");
    client.cancel().await.ok();
    tools
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect()
}
