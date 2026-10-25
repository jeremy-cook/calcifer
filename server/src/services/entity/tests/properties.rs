use super::*;

#[tokio::test]
async fn create_rejects_unknown_select_value() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());

    let err = try_create(
        &svc,
        with_select(todo("Water plants"), "todo.priority", "urgent"),
    )
    .await
    .expect_err("unknown priority should be rejected");

    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("urgent"), "{}", err.message());
    assert_eq!(entity_count(&pool).await, 0);
}

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

// I-24: a type the registry doesn't declare can't create, resolve or filter.
#[tokio::test]
async fn unknown_structure_types_are_rejected() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    create(&svc, note("Kept")).await;

    for structure_type in ["Custom", ""] {
        let err = try_create(&svc, untitled(structure_type))
            .await
            .expect_err("create of an unknown type");
        assert_eq!(
            err.code(),
            tonic::Code::InvalidArgument,
            "{structure_type:?}"
        );
    }
    let err = resolve(&svc, "Custom", by_name("Anything"), true)
        .await
        .expect_err("resolve of an unknown type");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(entity_count(&pool).await, 1);

    let err = svc
        .list_entities(Request::new(ListEntitiesRequest {
            structure_type: "Custom".to_string(),
        }))
        .await
        .expect_err("list of an unknown type");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    // An empty filter still lists every structure.
    let all = svc
        .list_entities(Request::new(ListEntitiesRequest {
            structure_type: String::new(),
        }))
        .await
        .expect("list all")
        .into_inner();
    assert_eq!(all.entities.len(), 1);
}

#[tokio::test]
async fn create_rejects_note_in_todo_tags() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let urgent = create(&svc, tag("urgent")).await;
    let other = create(&svc, note("Not a tag")).await;

    let err = try_create(
        &svc,
        with_relations(
            todo("Water plants"),
            "tags",
            &[(&urgent.id, "Tag"), (&other.id, "Note")],
        ),
    )
    .await
    .expect_err("a Note in tags should be rejected");

    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("tags"), "{}", err.message());
    assert_eq!(entity_count(&pool).await, 2);
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
