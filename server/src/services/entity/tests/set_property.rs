use super::*;

// I-11's done-when: two writers start from the same snapshot and each edit a
// different property; both edits survive.
#[tokio::test]
async fn concurrent_set_property_edits_both_survive() {
    let svc = entity_service(memory_pool().await);
    let snapshot = create(&svc, todo("Water plants")).await;

    set_property(&svc, &snapshot.id, "todo.status", select("done"))
        .await
        .expect("writer A");
    set_property(&svc, &snapshot.id, "todo.priority", select("high"))
        .await
        .expect("writer B");

    let stored = svc.load_entity(&snapshot.id).await.expect("load");
    assert_eq!(
        select_value(&stored, "todo.status").as_deref(),
        Some("done")
    );
    assert_eq!(
        select_value(&stored, "todo.priority").as_deref(),
        Some("high")
    );
}

// I-40: two writers race on one entity. Each reads before it writes, so a
// DEFERRED transaction that loses would fail at once with SQLITE_BUSY
// (Internal) instead of waiting on busy_timeout. Both must succeed. A few
// rounds, because one race can miss the window.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn racing_set_property_writes_both_succeed() {
    let scratch = ScratchDb::new().await;
    let svc = entity_service(scratch.pool.clone());
    let entity = create(&svc, todo("Water plants")).await;

    for (status, priority) in [("done", "high"), ("open", "low"), ("done", "medium")] {
        let (a, b) = tokio::join!(
            set_property(&svc, &entity.id, "todo.status", select(status)),
            set_property(&svc, &entity.id, "todo.priority", select(priority)),
        );
        a.expect("status write");
        b.expect("priority write");

        let stored = svc.load_entity(&entity.id).await.expect("load");
        assert_eq!(
            select_value(&stored, "todo.status").as_deref(),
            Some(status)
        );
        assert_eq!(
            select_value(&stored, "todo.priority").as_deref(),
            Some(priority)
        );
    }
    scratch.pool.close().await;
}

#[tokio::test]
async fn set_property_rejects_unknown_select_value() {
    let svc = entity_service(memory_pool().await);
    let entity = create(&svc, todo("Water plants")).await;

    let err = set_property(&svc, &entity.id, "todo.status", select("banana"))
        .await
        .expect_err("unknown status should be rejected");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("banana"), "{}", err.message());

    // A select on a property not declared as one is rejected too.
    let err = set_property(&svc, &entity.id, "todo.due", select("open"))
        .await
        .expect_err("select on a non-select property should be rejected");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    let stored = svc.load_entity(&entity.id).await.expect("load");
    assert_eq!(stored.properties, entity.properties);
    assert_eq!(stored.updated_at, entity.updated_at);
}

// I-16: every declared property takes only its declared kind.
#[tokio::test]
async fn values_must_match_the_declared_kind() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let entity = create(&svc, todo("Water plants")).await;

    // A `date` value on a select, through SetProperty and Create.
    let err = set_property(&svc, &entity.id, "todo.status", date("2026-06-13"))
        .await
        .expect_err("date on a select");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    let err = try_create(
        &svc,
        with_value(
            todo("Dated"),
            "todo.priority",
            property_value::Value::Date("2026-06-13".to_string()),
        ),
    )
    .await
    .expect_err("date on a select");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    // A `text` value on a declared date.
    let err = set_property(
        &svc,
        &entity.id,
        "todo.due",
        Some(property_value::Value::Text("tomorrow".to_string())),
    )
    .await
    .expect_err("text on a date property");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    let stored = svc.load_entity(&entity.id).await.expect("load");
    assert_eq!(stored.properties, entity.properties);
    assert_eq!(stored.links, entity.links);
    assert_eq!(entity_count(&pool).await, 1);

    // The matching kind is accepted: a date on `due`.
    set_property(&svc, &entity.id, "todo.due", date("2026-06-13"))
        .await
        .expect("date on due");
}

// I-22: a rich-text property's document is addressed by (entity id, declared
// property id), so the property itself takes no value of any kind.
#[tokio::test]
async fn rich_text_properties_take_no_value() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let target = create(&svc, note("Target")).await;
    let entity = create(&svc, todo("Water plants")).await;

    for value in [
        property_value::Value::Text("hello".to_string()),
        tags_value(&[(&target.id, "Note")]).expect("relations value"),
    ] {
        let err = set_property(&svc, &entity.id, "content", Some(value.clone()))
            .await
            .expect_err("SetProperty value on a rich-text property");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("rich-text"), "{}", err.message());

        let err = try_create(&svc, with_value(note("Sneaky"), "content", value))
            .await
            .expect_err("Create value on a rich-text property");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("rich-text"), "{}", err.message());
    }

    assert_eq!(entity_count(&pool).await, 2);
    assert!(stored_property_ids(&pool, &entity.id)
        .await
        .iter()
        .all(|id| id != "content"));
    assert!(stored_property_ids(&pool, &target.id).await.is_empty());
    // Clearing it is a harmless no-op, as on any property.
    set_property(&svc, &entity.id, "content", None)
        .await
        .expect("clear content");
}

#[tokio::test]
async fn set_property_checks_relation_targets() {
    let svc = entity_service(memory_pool().await);
    let urgent = create(&svc, tag("urgent")).await;
    let other = create(&svc, note("Not a tag")).await;
    let entity = create(&svc, todo("Water plants")).await;

    let err = set_property(
        &svc,
        &entity.id,
        "tags",
        tags_value(&[(&urgent.id, "Tag"), (&other.id, "Note")]),
    )
    .await
    .expect_err("a Note in tags should be rejected");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("tags"), "{}", err.message());
    let stored = svc.load_entity(&entity.id).await.expect("load");
    assert_eq!(stored.properties, entity.properties);
    assert!(link_targets(&stored, "tags").is_empty());

    // An empty claimed type is allowed; the link records the real one.
    let updated = set_property(&svc, &entity.id, "tags", tags_value(&[(&urgent.id, "")]))
        .await
        .expect("a Tag in tags");
    assert_eq!(
        link_targets(&updated, "tags"),
        [(urgent.id.clone(), "Tag".to_string())]
    );
}

#[tokio::test]
async fn set_property_clear_removes_only_that_property_and_its_links() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let urgent = create(&svc, tag("urgent")).await;
    let mentioned = create(&svc, note("Mentioned")).await;
    let entity = create(
        &svc,
        with_relations(todo("Water plants"), "tags", &[(&urgent.id, "Tag")]),
    )
    .await;
    // Content that mentions a note, for content-derived links to keep.
    let doc = format!(
        r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{{"type":"mention","attrs":{{"id":"{}","structureType":"Note"}}}}]}}]}}"#,
        mentioned.id
    );
    richtext_service(pool.clone(), WatchHub::new())
        .put_rich_text(Request::new(PutRichTextRequest {
            entity_id: entity.id.clone(),
            property_id: "content".to_string(),
            doc,
            expected_updated_at: None,
        }))
        .await
        .expect("put content");
    let entity = svc.load_entity(&entity.id).await.expect("load");
    assert_eq!(link_targets(&entity, "tags").len(), 1);
    assert_eq!(link_targets(&entity, "content").len(), 1);

    let cleared = set_property(&svc, &entity.id, "tags", None)
        .await
        .expect("clear tags");

    assert!(!cleared.properties.contains_key("tags"));
    assert!(link_targets(&cleared, "tags").is_empty());
    let mut expected = entity.properties.clone();
    expected.remove("tags");
    assert_eq!(cleared.properties, expected);
    assert_eq!(
        link_targets(&cleared, "content"),
        [(mentioned.id.clone(), "Note".to_string())]
    );

    // A value with no case clears too.
    set_property(&svc, &entity.id, "todo.status", None)
        .await
        .expect("clear status");
    let stored = svc.load_entity(&entity.id).await.expect("load");
    assert_eq!(select_value(&stored, "todo.status"), None);
    assert_eq!(
        select_value(&stored, "todo.priority").as_deref(),
        Some("none")
    );
}

#[tokio::test]
async fn set_property_missing_entity_is_not_found() {
    let svc = entity_service(memory_pool().await);

    let err = set_property(&svc, "no-such-entity", "todo.status", select("done"))
        .await
        .expect_err("missing entity");

    assert_eq!(err.code(), tonic::Code::NotFound);
}
