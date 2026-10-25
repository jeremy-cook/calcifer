use super::*;

#[tokio::test]
async fn select_values_are_checked_against_the_declared_kind() {
    let svc = entity_service(memory_pool().await);

    // A select on a property the structure doesn't declare as one.
    let err = try_create(&svc, with_select(note("Stray"), "status", "open"))
        .await
        .expect_err("select on a non-select property should be rejected");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    // A declared select holding some other kind of value.
    let mistyped = with_value(
        todo("Mistyped"),
        "todo.status",
        property_value::Value::Text("open".to_string()),
    );
    let err = try_create(&svc, mistyped)
        .await
        .expect_err("text on a select property should be rejected");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn tag_in_todo_tags_links_with_its_real_type() {
    let svc = entity_service(memory_pool().await);
    let urgent = create(&svc, tag("urgent")).await;

    // An empty claimed type is allowed; the link still records the real one.
    let todo = create(
        &svc,
        with_relations(todo("Water plants"), "tags", &[(&urgent.id, "")]),
    )
    .await;

    assert_eq!(
        link_targets(&todo, "tags"),
        [(urgent.id.clone(), "Tag".to_string())]
    );
}

#[tokio::test]
async fn relation_ref_with_wrong_claimed_type_is_rejected() {
    let svc = entity_service(memory_pool().await);
    let target = create(&svc, note("Target")).await;

    // Claims a Tag in a declared Tag-only property, but the target is a Note.
    let todo = create(&svc, todo("Water plants")).await;
    let err = set_property(&svc, &todo.id, "tags", tags_value(&[(&target.id, "Tag")]))
        .await
        .expect_err("lying claimed type should be rejected");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

// I-79: a property is declared in the registry before it takes a value, on
// Create and SetProperty, whatever the value's kind.
#[tokio::test]
async fn undeclared_properties_are_invalid() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let target = create(&svc, note("Target")).await;
    let memo = create(&svc, note("Memo")).await;
    let mut rx = svc.hub.subscribe();

    let values = [
        property_value::Value::Text("kept".to_string()),
        property_value::Value::Number(3.0),
        property_value::Value::Date("2026-02-03".to_string()),
        property_value::Value::Select("open".to_string()),
        property_value::Value::Relation(EntityRef {
            id: target.id.clone(),
            structure_type: "Note".to_string(),
        }),
        tags_value(&[(&target.id, "Note")]).expect("relations value"),
    ];
    // `TODO.note` only looks like the todo facet's namespace: ids are
    // case-sensitive, so it's undeclared, not an unattached facet property.
    for property_id in ["mood", "TODO.note"] {
        for value in &values {
            let err = try_create(&svc, with_value(note("M"), property_id, value.clone()))
                .await
                .expect_err("undeclared on create");
            assert_eq!(err.code(), tonic::Code::InvalidArgument, "{err:?}");
            assert!(
                err.message().contains("isn't a declared property"),
                "{}",
                err.message()
            );

            let err = set_property(&svc, &memo.id, property_id, Some(value.clone()))
                .await
                .expect_err("undeclared on set");
            assert_eq!(err.code(), tonic::Code::InvalidArgument, "{err:?}");
        }
    }

    assert_eq!(entity_count(&pool).await, 2);
    assert!(stored_property_ids(&pool, &memo.id).await.is_empty());
    assert_eq!(svc.load_entity(&memo.id).await.expect("load"), memo);
    assert!(rx.try_recv().is_err(), "no event");
}

// A value stored before undeclared properties were refused (I-79) can still
// be cleared, and clearing a relation value drops its links.
#[tokio::test]
async fn an_undeclared_value_can_be_cleared() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let target = create(&svc, note("Target")).await;
    let memo = create(&svc, note("Memo")).await;
    store_legacy_relations(&pool, &memo.id, "related", &target).await;
    assert_eq!(
        link_targets(&svc.load_entity(&memo.id).await.expect("load"), "related").len(),
        1
    );

    let cleared = set_property(&svc, &memo.id, "related", None)
        .await
        .expect("clear");

    assert!(!cleared.properties.contains_key("related"));
    assert!(link_targets(&cleared, "related").is_empty());
    assert!(stored_property_ids(&pool, &memo.id).await.is_empty());
}

// A client may resend a stale tag list that still holds a deleted Tag (Delete
// strips it from stored values, but not from a client's copy), so a dead ref
// must not block the write.
#[tokio::test]
async fn dead_relation_ref_does_not_block_set_property() {
    let svc = entity_service(memory_pool().await);
    let gone = create(&svc, tag("gone")).await;
    let kept = create(&svc, tag("kept")).await;
    let added = create(&svc, tag("added")).await;
    let todo = create(
        &svc,
        with_relations(
            todo("Water plants"),
            "tags",
            &[(&gone.id, "Tag"), (&kept.id, "Tag")],
        ),
    )
    .await;
    svc.delete_entity(Request::new(DeleteEntityRequest {
        id: gone.id.clone(),
    }))
    .await
    .expect("delete tag");

    let updated = set_property(
        &svc,
        &todo.id,
        "tags",
        tags_value(&[(&gone.id, "Tag"), (&kept.id, "Tag"), (&added.id, "Tag")]),
    )
    .await
    .expect("tag edit with a dead tag ref");

    let mut expected = vec![
        (kept.id.clone(), "Tag".to_string()),
        (added.id.clone(), "Tag".to_string()),
    ];
    expected.sort();
    assert_eq!(link_targets(&updated, "tags"), expected);
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
