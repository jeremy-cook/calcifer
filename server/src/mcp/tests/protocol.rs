use super::*;

#[tokio::test]
async fn json_rpc_speaks_mcp() {
    let f = fixture().await;
    let rpc = |message: Value| handle_message(&f.k, message);

    let init = rpc(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } }))
        .await
        .unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(init["result"]["serverInfo"]["name"], "calcifer");
    let unknown_version = rpc(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "1999-01-01" } }))
        .await
        .unwrap();
    assert_eq!(
        unknown_version["result"]["protocolVersion"],
        PROTOCOL_VERSIONS[0]
    );

    assert!(
        rpc(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .await
            .is_none()
    );

    let listed = rpc(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }))
        .await
        .unwrap();
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "search_notes",
            "get_note",
            "query_text_location",
            "edit_note",
            "create_note",
            "append_to_note",
            "tag_entity",
            "link_notes",
            "unlink_notes",
            "get_backlinks",
            "create_daily_note",
            "append_to_daily_note",
            "list_structures",
        ]
    );

    let call = |name: &str, arguments: Value| {
        rpc(
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": name, "arguments": arguments } }),
        )
    };
    call("create_note", json!({ "name": "B", "markdown": "" }))
        .await
        .unwrap();
    let created = call(
        "create_note",
        json!({ "name": "A", "markdown": "See [[B]]." }),
    )
    .await
    .unwrap();
    assert_eq!(created["result"]["isError"], false);
    assert!(created["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .starts_with("Saved note \"A\""));
    let tagged = call(
        "create_note",
        json!({ "name": "C", "markdown": "", "tags": ["ml"] }),
    )
    .await
    .unwrap();
    assert!(
        tagged["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .ends_with("tags: 1."),
        "{tagged}"
    );

    let failed = call("append_to_note", json!({ "name": "Nope", "markdown": "x" }))
        .await
        .unwrap();
    assert_eq!(
        failed["result"]["isError"], true,
        "a failing tool is a result, not a protocol error"
    );

    assert_eq!(
        call("no_such_tool", json!({})).await.unwrap()["error"]["code"],
        INVALID_PARAMS
    );
    assert_eq!(
        call("get_note", json!({ "name": 5 })).await.unwrap()["error"]["code"],
        INVALID_PARAMS
    );
    let start_zero = call(
        "edit_note",
        json!({ "name": "A", "start": 0, "end": 0, "markdown": "", "version": 0 }),
    )
    .await
    .unwrap();
    assert_eq!(start_zero["result"]["isError"], true, "{start_zero}");
    let not_an_object = rpc(json!(42)).await.unwrap();
    assert_eq!(
        (&not_an_object["id"], &not_an_object["error"]["code"]),
        (&Value::Null, &json!(INVALID_REQUEST))
    );
    assert_eq!(
        rpc(json!({ "jsonrpc": "2.0", "id": 4, "method": "nope" }))
            .await
            .unwrap()["error"]["code"],
        METHOD_NOT_FOUND
    );
}

#[tokio::test]
async fn json_rpc_tag_entity_tags_a_note() {
    let f = fixture().await;
    let call = |name: &str, arguments: Value| {
        handle_message(
            &f.k,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } }),
        )
    };
    let text = |reply: &Value| {
        reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string()
    };

    call("create_note", json!({ "name": "A", "markdown": "Body." }))
        .await
        .unwrap();
    let tagged = call(
        "tag_entity",
        json!({ "name": "A", "add": ["ml", "papers"] }),
    )
    .await
    .unwrap();
    assert_eq!(tagged["result"]["isError"], false, "{tagged}");
    assert_eq!(text(&tagged), "Tags on \"A\": ml, papers.");
    let untagged = call("tag_entity", json!({ "name": "A", "remove": ["ml"] }))
        .await
        .unwrap();
    assert_eq!(text(&untagged), "Tags on \"A\": papers.");

    let none_given = call("tag_entity", json!({ "name": "A" })).await.unwrap();
    assert_eq!(none_given["result"]["isError"], true, "{none_given}");
    let no_name = call("tag_entity", json!({ "add": ["ml"] })).await.unwrap();
    assert_eq!(no_name["error"]["code"], INVALID_PARAMS);
    let read = call("get_note", json!({ "name": "A" })).await.unwrap();
    assert!(text(&read).contains("\ntags: [\"papers\"]\n"), "{read}");
}

#[tokio::test]
async fn json_rpc_unlink_notes_removes_a_relationship() {
    let f = fixture().await;
    let call = |name: &str, arguments: Value| {
        handle_message(
            &f.k,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } }),
        )
    };
    let text = |reply: &Value| {
        reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string()
    };

    call("create_note", json!({ "name": "A", "markdown": "Body." }))
        .await
        .unwrap();
    call("create_note", json!({ "name": "B", "markdown": "" }))
        .await
        .unwrap();
    let linked = call("link_notes", json!({ "from": "A", "to": "B" }))
        .await
        .unwrap();
    assert_eq!(linked["result"]["isError"], false, "{linked}");
    assert_eq!(text(&linked), "Related \"A\" -> \"B\".");
    let read = call("get_note", json!({ "name": "A" })).await.unwrap();
    assert!(text(&read).contains("\nrelated: [\"B\"]\n"), "{read}");

    let unlinked = call("unlink_notes", json!({ "from": "A", "to": "B" }))
        .await
        .unwrap();
    assert_eq!(unlinked["result"]["isError"], false, "{unlinked}");
    assert_eq!(text(&unlinked), "Unrelated \"A\" -> \"B\".");
    let again = call("unlink_notes", json!({ "from": "A", "to": "B" }))
        .await
        .unwrap();
    assert_eq!(again["result"]["isError"], false, "{again}");
    assert_eq!(text(&again), "\"A\" wasn't related to \"B\".");
    let read = call("get_note", json!({ "name": "A" })).await.unwrap();
    assert!(text(&read).contains("\nrelated: []\n"), "{read}");

    let missing = call("unlink_notes", json!({ "from": "A", "to": "Ghost" }))
        .await
        .unwrap();
    assert_eq!(missing["result"]["isError"], true, "{missing}");
    assert!(
        text(&missing).starts_with("No note named or with id"),
        "{missing}"
    );
    let no_to = call("unlink_notes", json!({ "from": "A" })).await.unwrap();
    assert_eq!(no_to["error"]["code"], INVALID_PARAMS);
}

#[tokio::test]
async fn the_router_answers_only_this_machine() {
    let f = fixture().await;
    let (host, json) = (
        ("host", "localhost:8081"),
        ("content-type", "application/json"),
    );

    // Claude Code: no Origin, a JSON body.
    assert_eq!(post_status(&f, &[host, json]).await, StatusCode::OK);
    let inspector = ("origin", "http://localhost:6274");
    assert_eq!(
        post_status(&f, &[host, json, inspector]).await,
        StatusCode::OK
    );

    let rebound = ("host", "evil.example:8081");
    assert_eq!(
        post_status(&f, &[rebound, json]).await,
        StatusCode::FORBIDDEN
    );
    let foreign = ("origin", "https://evil.example");
    assert_eq!(
        post_status(&f, &[host, json, foreign]).await,
        StatusCode::FORBIDDEN
    );
    // A no-cors form post from another site.
    let text = ("content-type", "text/plain");
    assert_eq!(
        post_status(&f, &[host, text]).await,
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
}
