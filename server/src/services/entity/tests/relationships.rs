use super::*;

#[tokio::test]
async fn add_relationship_writes_the_row_and_a_related_link() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let a = create(&svc, note("A")).await;
    let b = create(&svc, note("B")).await;
    let mut rx = svc.hub.subscribe();

    let updated = add_rel(&svc, &a.id, &b.id).await.expect("add");

    assert_eq!(
        link_targets(&updated, RELATED_SCOPE),
        [(b.id.clone(), "Note".to_string())]
    );
    assert_eq!(relationship_row_count(&pool).await, 1);
    assert!(millis(updated.updated_at) > millis(a.updated_at));
    assert_eq!(svc.load_entity(&a.id).await.expect("load"), updated);
    assert_eq!(only_upsert(&mut rx), updated);

    let backlinks = list_backlinks_of(&svc, &b.id).await.expect("backlinks");
    assert_eq!(
        backlink_rows(&backlinks)
            .into_iter()
            .map(|(id, scope, _)| (id, scope))
            .collect::<Vec<_>>(),
        [(a.id.clone(), "@related".to_string())]
    );
}

#[tokio::test]
async fn add_relationship_twice_is_a_no_op() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let a = create(&svc, note("A")).await;
    let b = create(&svc, note("B")).await;
    let related = add_rel(&svc, &a.id, &b.id).await.expect("add");
    let mut rx = svc.hub.subscribe();

    let same = add_rel(&svc, &a.id, &b.id).await.expect("add again");

    assert_eq!(same, related);
    assert_eq!(millis(same.updated_at), millis(related.updated_at));
    assert_eq!(relationship_row_count(&pool).await, 1);
    assert!(rx.try_recv().is_err(), "no event");
}

#[tokio::test]
async fn add_relationship_checks_in_order() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let a = create(&svc, note("A")).await;
    let b = create(&svc, note("B")).await;
    let urgent = create(&svc, tag("urgent")).await;
    let mut rx = svc.hub.subscribe();

    let unknown_source = add_rel(&svc, "missing", &b.id).await.expect_err("source");
    assert_eq!(unknown_source.code(), tonic::Code::NotFound);

    // A Tag can't hold relationships, and that check comes before the target's.
    let tag_unknown_target = add_rel(&svc, &urgent.id, "missing")
        .await
        .expect_err("tag, unknown target");
    assert_eq!(tag_unknown_target.code(), tonic::Code::FailedPrecondition);
    let tag_source = add_rel(&svc, &urgent.id, &a.id).await.expect_err("tag");
    assert_eq!(tag_source.code(), tonic::Code::FailedPrecondition);

    let unknown_target = add_rel(&svc, &a.id, "missing").await.expect_err("target");
    assert_eq!(unknown_target.code(), tonic::Code::NotFound);

    let to_self = add_rel(&svc, &a.id, &a.id).await.expect_err("self");
    assert_eq!(to_self.code(), tonic::Code::InvalidArgument);

    let to_tag = add_rel(&svc, &a.id, &urgent.id)
        .await
        .expect_err("Note->Tag");
    assert_eq!(to_tag.code(), tonic::Code::InvalidArgument);

    for (entity_id, target_id) in [("", b.id.as_str()), (a.id.as_str(), ""), ("", "")] {
        let err = add_rel(&svc, entity_id, target_id)
            .await
            .expect_err("empty id");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    // The current registry decides, not what allowed a row when it was written:
    // an existing Note -> Tag row doesn't make the same add a no-op.
    insert_relationship_row(&pool, &a.id, &urgent.id).await;
    let stale = add_rel(&svc, &a.id, &urgent.id).await.expect_err("stale");
    assert_eq!(stale.code(), tonic::Code::InvalidArgument);

    assert_eq!(
        relationship_row_count(&pool).await,
        1,
        "only the direct row"
    );
    assert!(rx.try_recv().is_err(), "no event");
}

#[tokio::test]
async fn remove_relationship_drops_the_row_and_link() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let a = create(&svc, note("A")).await;
    let b = create(&svc, note("B")).await;
    let c = create(&svc, note("C")).await;
    add_rel(&svc, &a.id, &b.id).await.expect("add b");
    let both = add_rel(&svc, &a.id, &c.id).await.expect("add c");
    let mut rx = svc.hub.subscribe();

    let updated = remove_rel(&svc, &a.id, &b.id).await.expect("remove");

    assert_eq!(
        link_targets(&updated, RELATED_SCOPE),
        [(c.id.clone(), "Note".to_string())]
    );
    assert_eq!(relationship_row_count(&pool).await, 1);
    assert!(millis(updated.updated_at) > millis(both.updated_at));
    assert_eq!(svc.load_entity(&a.id).await.expect("load"), updated);
    assert_eq!(only_upsert(&mut rx), updated);
    let backlinks = list_backlinks_of(&svc, &b.id).await.expect("backlinks");
    assert!(backlinks.is_empty());
}

#[tokio::test]
async fn remove_relationship_absent_is_a_no_op() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let a = create(&svc, note("A")).await;
    let b = create(&svc, note("B")).await;
    let c = create(&svc, note("C")).await;
    let related = add_rel(&svc, &a.id, &b.id).await.expect("add");
    let mut rx = svc.hub.subscribe();

    // A real note it isn't related to, an id that doesn't exist, and a note with
    // no relationships at all.
    let missing = "missing".to_string();
    for (before, target_id) in [(&related, &c.id), (&related, &missing), (&b, &a.id)] {
        let same = remove_rel(&svc, &before.id, target_id)
            .await
            .expect("remove absent");
        assert_eq!(&same, before);
    }

    assert_eq!(relationship_row_count(&pool).await, 1);
    assert!(rx.try_recv().is_err(), "no event");

    let unknown_source = remove_rel(&svc, "missing", &b.id)
        .await
        .expect_err("source");
    assert_eq!(unknown_source.code(), tonic::Code::NotFound);
    let empty = remove_rel(&svc, &a.id, "").await.expect_err("empty id");
    assert_eq!(empty.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn remove_relationship_ignores_relates_to() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let urgent = create(&svc, tag("urgent")).await;
    let a = create(&svc, note("A")).await;
    // A row a narrowed `relates_to` would leave behind: Tag relates to nothing.
    insert_relationship_row(&pool, &urgent.id, &a.id).await;

    let updated = remove_rel(&svc, &urgent.id, &a.id).await.expect("remove");

    assert!(link_targets(&updated, RELATED_SCOPE).is_empty());
    assert_eq!(relationship_row_count(&pool).await, 0);
}

#[tokio::test]
async fn relationship_writes_keep_content_links() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let a = create(&svc, note("A")).await;
    let b = create(&svc, note("B")).await;
    let mention_b = mention("mention", &b);
    put_content(&pool, &a.id, std::slice::from_ref(&mention_b)).await;
    let linked = [(b.id.clone(), "Note".to_string())];

    let related = add_rel(&svc, &a.id, &b.id).await.expect("add");
    assert_eq!(link_targets(&related, "content"), linked);
    assert_eq!(link_targets(&related, RELATED_SCOPE), linked);

    // Removing the relationship keeps the mention's link.
    let unrelated = remove_rel(&svc, &a.id, &b.id).await.expect("remove");
    assert_eq!(link_targets(&unrelated, "content"), linked);
    assert!(link_targets(&unrelated, RELATED_SCOPE).is_empty());

    // A rich-text save keeps the relationship's link, even one that drops the mention.
    add_rel(&svc, &a.id, &b.id).await.expect("add again");
    put_content(&pool, &a.id, &[]).await;
    let saved = svc.load_entity(&a.id).await.expect("load");
    assert!(link_targets(&saved, "content").is_empty());
    assert_eq!(link_targets(&saved, RELATED_SCOPE), linked);
}

#[tokio::test]
async fn deleting_a_target_republishes_its_relationship_sources() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let b = create(&svc, note("B")).await;
    // `a` also holds a `related` property value naming B, stored before I-79
    // (no structure declares a relation to a Note), so `strip_relation_refs`
    // changes it too: it's published once, not twice.
    let a = create(&svc, todo("A")).await;
    store_legacy_relations(&pool, &a.id, "related", &b).await;
    let c = create(&svc, note("C")).await;
    let other = create(&svc, note("Other")).await;
    add_rel(&svc, &a.id, &b.id).await.expect("a -> b");
    add_rel(&svc, &c.id, &b.id).await.expect("c -> b");
    add_rel(&svc, &c.id, &other.id).await.expect("c -> other");
    let a_before = svc.load_entity(&a.id).await.expect("load a");
    let c_before = svc.load_entity(&c.id).await.expect("load c");
    let mut rx = svc.hub.subscribe();

    delete(&svc, &b.id).await;

    let first = rx.recv().await.expect("deleted event");
    assert_eq!(
        first.event,
        Some(watch_entities_response::Event::DeletedId(b.id.clone()))
    );
    let mut expected = [(&a_before, &a), (&c_before, &c)];
    expected.sort_by_key(|(_, e)| e.id.clone());
    for (before, source) in expected {
        let event = rx.recv().await.expect("upserted event");
        let Some(watch_entities_response::Event::Upserted(upserted)) = event.event else {
            panic!("expected an upsert, got {event:?}");
        };
        assert_eq!(upserted.id, source.id);
        assert!(millis(upserted.updated_at) > millis(before.updated_at));
        assert!(
            !upserted
                .links
                .iter()
                .any(|l| l.target.as_ref().is_some_and(|t| t.id == b.id)),
            "no link to the deleted target is left"
        );
        assert_eq!(upserted, svc.load_entity(&source.id).await.expect("load"));
    }
    assert!(rx.try_recv().is_err(), "no further events");

    // C's relationship to the other note is untouched.
    let c_after = svc.load_entity(&c.id).await.expect("load c");
    assert_eq!(
        link_targets(&c_after, RELATED_SCOPE),
        [(other.id.clone(), "Note".to_string())]
    );
    assert_eq!(relationship_row_count(&pool).await, 1);
}

#[tokio::test]
async fn deleting_a_source_removes_its_relationships() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let a = create(&svc, note("A")).await;
    let b = create(&svc, note("B")).await;
    add_rel(&svc, &a.id, &b.id).await.expect("add");
    let mut rx = svc.hub.subscribe();

    delete(&svc, &a.id).await;

    assert_eq!(relationship_row_count(&pool).await, 0);
    let first = rx.recv().await.expect("deleted event");
    assert_eq!(
        first.event,
        Some(watch_entities_response::Event::DeletedId(a.id.clone()))
    );
    assert!(rx.try_recv().is_err(), "the target isn't republished");
    let b_after = svc.load_entity(&b.id).await.expect("load b");
    assert_eq!(b_after, b);
}

#[tokio::test]
async fn set_facet_property_without_the_facet_is_failed_precondition() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let ship = create(&svc, note("Ship")).await;

    let err = set_property(&svc, &ship.id, "todo.status", select("done"))
        .await
        .expect_err("todo isn't attached");

    assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    assert!(
        err.message()
            .contains("attach the todo facet before setting todo.status"),
        "{}",
        err.message()
    );
    assert!(stored_property_ids(&pool, &ship.id).await.is_empty());
}
