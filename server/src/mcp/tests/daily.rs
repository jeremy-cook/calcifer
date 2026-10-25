use super::*;

#[tokio::test]
async fn daily_notes_are_one_per_day() {
    let f = fixture().await;
    f.k.create_note("Transformers", "x", None).await.unwrap();
    let created = f.k.create_daily_note("2026-06-13").await.unwrap();
    assert!(
        created.contains("\"June 13, 2026\"") && created.ends_with("for 2026-06-13 created."),
        "{created}"
    );
    let again = f.k.create_daily_note("2026-06-13").await.unwrap();
    assert!(
        again.ends_with("for 2026-06-13 already existed."),
        "{again}"
    );

    f.k.append_to_daily_note("2026-06-13", "Read about [[Transformers]] today.")
        .await
        .unwrap();
    assert!(f
        .k
        .get_backlinks("Transformers")
        .await
        .unwrap()
        .contains("June 13, 2026"));
}

#[tokio::test]
async fn get_note_by_date_reads_the_daily_note() {
    let f = fixture().await;
    f.k.append_to_daily_note("2026-10-24", "Met the team.\n\nShipped it.")
        .await
        .unwrap();

    let note = f.k.get_note(NoteKey::Date("2026-10-24")).await.unwrap();
    assert_eq!(
        frontmatter_keys(&note),
        ["name", "version", "date"],
        "{note}"
    );
    assert!(
        note.starts_with("---\nname: \"October 24, 2026\"\nversion: "),
        "{note}"
    );
    assert!(note.contains("\ndate: \"2026-10-24\"\n"), "{note}");
    assert!(
        !note.contains("tags") && !note.contains("related"),
        "{note}"
    );
    assert!(version(&note) > 0, "{note}");
    assert_eq!(body(&note), "Met the team.\n\nShipped it.");
}

#[tokio::test]
async fn get_note_by_missing_date_creates_nothing() {
    let f = fixture().await;
    let before = f.entity_count().await;

    let failure = err(f.k.get_note(NoteKey::Date("2026-10-24")).await);
    assert_eq!(
        failure,
        "No daily note for 2026-10-24. Use append_to_daily_note to start one."
    );
    for failure in [
        err(f
            .k
            .query_text_location(NoteKey::Date("2026-10-24"), "x")
            .await),
        err(f
            .k
            .edit_note(NoteKey::Date("2026-10-24"), 1, 0, "x", 0)
            .await),
    ] {
        assert!(failure.contains("append_to_daily_note"), "{failure}");
    }

    assert_eq!(f.entity_count().await, before);
    let req = ResolveEntityRequest {
        structure_type: String::new(),
        key: Some(resolve_entity_request::Key::Date("2026-10-24".into())),
        create_if_missing: false,
    };
    let lookup = f.entity.resolve_entity(Request::new(req)).await;
    assert_eq!(lookup.unwrap_err().code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn a_malformed_date_fails_with_the_servers_message() {
    let f = fixture().await;
    let before = f.entity_count().await;

    for failure in [
        err(f.k.get_note(NoteKey::Date("tomorrow")).await),
        err(f
            .k
            .query_text_location(NoteKey::Date("tomorrow"), "x")
            .await),
        err(f.k.edit_note(NoteKey::Date("tomorrow"), 1, 0, "x", 0).await),
    ] {
        assert!(
            failure.contains("date must be an ISO calendar day (yyyy-MM-dd)"),
            "{failure}"
        );
    }
    assert_eq!(f.entity_count().await, before);
}

#[tokio::test]
async fn query_and_edit_a_daily_note_by_date() {
    let f = fixture().await;
    let day = NoteKey::Date;
    f.k.append_to_daily_note("2026-10-24", "First.\n\nSecond.\n\nThird.")
        .await
        .unwrap();

    let found =
        f.k.query_text_location(day("2026-10-24"), "Second")
            .await
            .unwrap();
    let v = version(&found);
    assert!(found.contains("lines 3-3 (1 match):\nSecond."), "{found}");
    assert!(f
        .k
        .query_text_location(day("2026-10-24"), "nowhere")
        .await
        .unwrap()
        .ends_with("in Daily note \"October 24, 2026\"."));

    let done =
        f.k.edit_note(day("2026-10-24"), 3, 3, "Replaced.", v)
            .await
            .unwrap();
    assert!(
        done.starts_with("Replaced lines 3-3 of Daily note \"October 24, 2026\" ("),
        "{done}"
    );
    let note = f.k.get_note(day("2026-10-24")).await.unwrap();
    assert_eq!(body(&note), "First.\n\nReplaced.\n\nThird.");
    assert_eq!(f.entity_count().await, 1, "still the one day");
}

#[tokio::test]
async fn note_key_needs_exactly_one_of_name_and_date() {
    let f = fixture().await;
    f.k.create_note("A", "Body.", None).await.unwrap();
    let call = |name: &str, arguments: Value| {
        handle_message(
            &f.k,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } }),
        )
    };
    let rest = |tool: &str| match tool {
        "get_note" => json!({}),
        "query_text_location" => json!({ "query": "x" }),
        _ => json!({ "start": 1, "end": 1, "markdown": "x", "version": 0 }),
    };

    for tool in ["get_note", "query_text_location", "edit_note"] {
        for key in [json!({ "name": "A", "date": "2026-10-24" }), json!({})] {
            let mut args = rest(tool);
            args.as_object_mut()
                .unwrap()
                .extend(key.as_object().unwrap().clone());
            let reply = call(tool, args).await.unwrap();
            assert!(reply.get("error").is_none(), "{tool}: {reply}");
            assert_eq!(reply["result"]["isError"], true, "{tool}: {reply}");
            let text = reply["result"]["content"][0]["text"].as_str().unwrap();
            assert!(
                text.starts_with("Give either name (a note) or date (a daily note"),
                "{tool}: {text}"
            );
        }
    }
    assert_eq!(f.entity_count().await, 1, "nothing was created");
}
