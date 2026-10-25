use super::*;

#[tokio::test]
async fn add_relation_target_appends_links_and_publishes() {
    let svc = entity_service(memory_pool().await);
    let a = create(&svc, tag("a")).await;
    let b = create(&svc, tag("b")).await;
    let entity = tagged_todo(&svc, &[&a]).await;
    let mut rx = svc.hub.subscribe();

    let updated = add_target(&svc, &entity.id, "tags", &b.id)
        .await
        .expect("add");

    assert_eq!(relation_refs(&updated, "tags"), tag_refs(&[&a, &b]));
    let mut linked = tag_refs(&[&a, &b]);
    linked.sort();
    assert_eq!(link_targets(&updated, "tags"), linked);
    assert!(millis(updated.updated_at) > millis(entity.updated_at));
    assert_eq!(svc.load_entity(&entity.id).await.expect("load"), updated);
    assert_eq!(only_upsert(&mut rx), updated);
}

#[tokio::test]
async fn add_relation_target_twice_is_a_no_op() {
    let svc = entity_service(memory_pool().await);
    let a = create(&svc, tag("a")).await;
    let entity = tagged_todo(&svc, &[&a]).await;
    let mut rx = svc.hub.subscribe();

    let same = add_target(&svc, &entity.id, "tags", &a.id)
        .await
        .expect("add again");

    assert_eq!(same, entity);
    assert_eq!(millis(same.updated_at), millis(entity.updated_at));
    assert!(rx.try_recv().is_err(), "no event");
}

// I-34's done-when at the RPC level: two writers add different targets to the
// same starting list, and neither add undoes the other.
#[tokio::test]
async fn two_adds_of_different_targets_both_survive() {
    let svc = entity_service(memory_pool().await);
    let seed = create(&svc, tag("seed")).await;
    let a = create(&svc, tag("a")).await;
    let b = create(&svc, tag("b")).await;
    let entity = tagged_todo(&svc, &[&seed]).await;

    add_target(&svc, &entity.id, "tags", &a.id)
        .await
        .expect("add a");
    add_target(&svc, &entity.id, "tags", &b.id)
        .await
        .expect("add b");

    let stored = svc.load_entity(&entity.id).await.expect("load");
    assert_eq!(relation_refs(&stored, "tags"), tag_refs(&[&seed, &a, &b]));
    assert_eq!(link_targets(&stored, "tags").len(), 3);
}

#[tokio::test]
async fn add_relation_target_checks_entity_property_and_target() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let a = create(&svc, tag("a")).await;
    let other = create(&svc, note("Not a tag")).await;
    let entity = create(&svc, note("Ship")).await;

    let missing = add_target(&svc, "missing", "tags", &a.id)
        .await
        .expect_err("unknown entity");
    assert_eq!(missing.code(), tonic::Code::NotFound);

    for (entity_id, property_id, target_id) in [
        ("", "tags", a.id.as_str()),
        (entity.id.as_str(), "", a.id.as_str()),
        (entity.id.as_str(), "tags", ""),
    ] {
        let err = add_target(&svc, entity_id, property_id, target_id)
            .await
            .expect_err("empty id");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    // Undeclared, or declared but not a `relations` property.
    for property_id in ["foo", "content"] {
        let err = add_target(&svc, &entity.id, property_id, &a.id)
            .await
            .expect_err("not a relations property");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(
            err.message().contains("is not a relations property"),
            "{}",
            err.message()
        );
    }
    let with_todo = create(&svc, with_facets(note("Todo"), &["todo"])).await;
    let status = add_target(&svc, &with_todo.id, "todo.status", &a.id)
        .await
        .expect_err("a select");
    assert_eq!(status.code(), tonic::Code::InvalidArgument);

    let unknown = add_target(&svc, &entity.id, "tags", "missing")
        .await
        .expect_err("unknown target");
    assert_eq!(unknown.code(), tonic::Code::NotFound);

    let wrong = add_target(&svc, &entity.id, "tags", &other.id)
        .await
        .expect_err("a Note isn't a Tag");
    assert_eq!(wrong.code(), tonic::Code::InvalidArgument);
    assert!(
        wrong.message().contains("must reference a Tag"),
        "{}",
        wrong.message()
    );

    assert!(stored_property_ids(&pool, &entity.id).await.is_empty());
    assert_eq!(svc.load_entity(&entity.id).await.expect("load"), entity);
}

// ADR 12, rule 9: no write may name a property id starting with `@`, since
// those are the link scopes the server derives.
#[tokio::test]
async fn reserved_property_ids_are_invalid_argument() {
    use crate::proto::rich_text_service_server::RichTextService as _;
    use crate::proto::GetRichTextRequest;

    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let rich = richtext_service(pool.clone(), WatchHub::new());
    let entity = create(&svc, note("Ship")).await;
    let target = create(&svc, note("Dock")).await;
    let before = entity_count(&pool).await;

    fn assert_reserved(err: &Status) {
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("reserved"), "{}", err.message());
    }

    let create_err = try_create(
        &svc,
        with_value(
            note("Sneaky"),
            "@x",
            property_value::Value::Text("v".into()),
        ),
    )
    .await
    .expect_err("CreateEntity with @x");
    assert_reserved(&create_err);
    assert_eq!(entity_count(&pool).await, before);

    let text = Some(property_value::Value::Text("v".into()));
    assert_reserved(
        &set_property(&svc, &entity.id, "@related", text)
            .await
            .expect_err("SetEntityProperty with @related"),
    );
    assert_reserved(
        &set_property(&svc, &entity.id, "@related", None)
            .await
            .expect_err("clearing @related"),
    );
    assert_reserved(
        &add_target(&svc, &entity.id, "@related", &target.id)
            .await
            .expect_err("AddRelationTarget with @related"),
    );
    assert_reserved(
        &remove_target(&svc, &entity.id, "@related", &target.id)
            .await
            .expect_err("RemoveRelationTarget with @related"),
    );

    assert_reserved(
        &rich
            .put_rich_text(Request::new(PutRichTextRequest {
                entity_id: entity.id.clone(),
                property_id: "@x".to_string(),
                doc: r#"{"type":"doc","content":[]}"#.to_string(),
                expected_updated_at: None,
            }))
            .await
            .expect_err("PutRichText with @x"),
    );
    assert_reserved(
        &rich
            .get_rich_text(Request::new(GetRichTextRequest {
                entity_id: entity.id.clone(),
                property_id: "@x".to_string(),
            }))
            .await
            .expect_err("GetRichText with @x"),
    );

    assert!(stored_property_ids(&pool, &entity.id).await.is_empty());
    assert_eq!(svc.load_entity(&entity.id).await.expect("load"), entity);
}

#[tokio::test]
async fn relation_target_on_unattached_facet_property_is_failed_precondition() {
    let svc = entity_service(memory_pool().await);
    let a = create(&svc, tag("a")).await;
    let ship = create(&svc, note("Ship")).await;
    let mut rx = svc.hub.subscribe();

    let add = add_target(&svc, &ship.id, "todo.status", &a.id)
        .await
        .expect_err("todo isn't attached");
    let remove = remove_target(&svc, &ship.id, "todo.status", &a.id)
        .await
        .expect_err("todo isn't attached");

    for err in [add, remove] {
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(
            err.message()
                .contains("attach the todo facet before editing todo.status"),
            "{}",
            err.message()
        );
    }
    assert!(rx.try_recv().is_err(), "no event");
}

#[tokio::test]
async fn remove_relation_target_drops_the_ref_and_its_link() {
    let svc = entity_service(memory_pool().await);
    let a = create(&svc, tag("a")).await;
    let b = create(&svc, tag("b")).await;
    let entity = tagged_todo(&svc, &[&a, &b]).await;
    let mut rx = svc.hub.subscribe();

    let updated = remove_target(&svc, &entity.id, "tags", &a.id)
        .await
        .expect("remove");

    assert_eq!(relation_refs(&updated, "tags"), tag_refs(&[&b]));
    assert_eq!(link_targets(&updated, "tags"), tag_refs(&[&b]));
    assert!(millis(updated.updated_at) > millis(entity.updated_at));
    assert_eq!(svc.load_entity(&entity.id).await.expect("load"), updated);
    assert_eq!(only_upsert(&mut rx), updated);
}

#[tokio::test]
async fn remove_last_relation_target_clears_the_property() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let a = create(&svc, tag("a")).await;
    let entity = tagged_todo(&svc, &[&a]).await;

    let updated = remove_target(&svc, &entity.id, "tags", &a.id)
        .await
        .expect("remove");

    assert!(!updated.properties.contains_key("tags"));
    assert!(link_targets(&updated, "tags").is_empty());
    assert!(!stored_property_ids(&pool, &entity.id)
        .await
        .contains(&"tags".to_string()));
}

#[tokio::test]
async fn remove_relation_target_absent_is_a_no_op() {
    let svc = entity_service(memory_pool().await);
    let a = create(&svc, tag("a")).await;
    let b = create(&svc, tag("b")).await;
    let entity = tagged_todo(&svc, &[&a]).await;
    let untagged = create(&svc, note("Untagged")).await;
    let mut rx = svc.hub.subscribe();

    // A real tag the note doesn't have, an id that doesn't exist, and a
    // note with no `tags` value at all.
    let missing = "missing".to_string();
    for (before, target_id) in [(&entity, &b.id), (&entity, &missing), (&untagged, &a.id)] {
        let same = remove_target(&svc, &before.id, "tags", target_id)
            .await
            .expect("remove absent");
        assert_eq!(&same, before);
    }

    assert!(rx.try_recv().is_err(), "no event");
}
