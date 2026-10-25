use super::*;

#[tokio::test]
async fn create_with_a_facet_attaches_it_with_defaults() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());

    let created = create(&svc, with_facets(note("Ship"), &["todo"])).await;

    assert_eq!(created.facets, vec!["todo".to_string()]);
    assert_eq!(
        select_value(&created, "todo.status").as_deref(),
        Some("open")
    );
    assert_eq!(
        select_value(&created, "todo.priority").as_deref(),
        Some("none")
    );
    assert!(!created.properties.contains_key("todo.due"));
    assert_eq!(facet_row_count(&pool).await, 1);
    assert_eq!(svc.load_entity(&created.id).await.expect("load"), created);
}

#[tokio::test]
async fn create_with_a_facet_lets_the_caller_set_its_values() {
    let svc = entity_service(memory_pool().await);

    let req = with_select(
        with_facets(note("Ship"), &["todo"]),
        "todo.priority",
        "high",
    );
    let created = create(&svc, req).await;

    assert_eq!(
        select_value(&created, "todo.status").as_deref(),
        Some("open")
    );
    assert_eq!(
        select_value(&created, "todo.priority").as_deref(),
        Some("high")
    );
}

#[tokio::test]
async fn create_with_a_value_for_an_unrequested_facet_is_failed_precondition() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());

    let err = try_create(&svc, with_select(note("Ship"), "todo.status", "done"))
        .await
        .expect_err("todo isn't requested");

    assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    assert!(
        err.message().contains("attach the todo facet"),
        "{}",
        err.message()
    );
    assert_eq!(entity_count(&pool).await, 0);
}

#[tokio::test]
async fn create_with_an_unknown_facet_is_invalid_argument() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());

    let err = try_create(&svc, with_facets(note("Ship"), &["chore"]))
        .await
        .expect_err("unknown facet");

    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("chore"), "{}", err.message());
    assert_eq!(entity_count(&pool).await, 0);
}

#[tokio::test]
async fn create_with_a_facet_the_structure_does_not_accept_is_failed_precondition() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());

    let err = try_create(&svc, with_facets(tag("urgent"), &["todo"]))
        .await
        .expect_err("a Tag doesn't accept todo");

    assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    assert_eq!(entity_count(&pool).await, 0);
}

#[tokio::test]
async fn attach_writes_select_defaults_and_publishes() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let ship = create(&svc, note("Ship")).await;
    let mut rx = svc.hub.subscribe();
    // So the bump shows: timestamps are in milliseconds.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    let attached = attach(&svc, &ship.id, "todo").await.expect("attach");

    assert_eq!(attached.facets, vec!["todo".to_string()]);
    assert_eq!(
        select_value(&attached, "todo.status").as_deref(),
        Some("open")
    );
    assert_eq!(
        select_value(&attached, "todo.priority").as_deref(),
        Some("none")
    );
    assert!(millis(attached.updated_at) > millis(ship.updated_at));
    assert_eq!(only_upsert(&mut rx), attached);
}

#[tokio::test]
async fn attach_twice_is_a_no_op() {
    let svc = entity_service(memory_pool().await);
    let ship = create(&svc, note("Ship")).await;
    let first = attach(&svc, &ship.id, "todo").await.expect("attach");
    let mut rx = svc.hub.subscribe();

    let second = attach(&svc, &ship.id, "todo").await.expect("attach again");

    assert_eq!(second, first);
    assert_eq!(second.updated_at, first.updated_at);
    assert!(rx.try_recv().is_err(), "no event");
}

#[tokio::test]
async fn attach_checks_the_entity_and_the_facet() {
    let svc = entity_service(memory_pool().await);
    let ship = create(&svc, note("Ship")).await;
    let label = create(&svc, tag("urgent")).await;

    let missing = attach(&svc, "missing", "todo").await.expect_err("missing");
    assert_eq!(missing.code(), tonic::Code::NotFound);
    let unknown = attach(&svc, &ship.id, "chore").await.expect_err("unknown");
    assert_eq!(unknown.code(), tonic::Code::InvalidArgument);
    let refused = attach(&svc, &label.id, "todo").await.expect_err("refused");
    assert_eq!(refused.code(), tonic::Code::FailedPrecondition);
}

#[tokio::test]
async fn detach_deletes_the_facet_values_and_publishes() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let ship = create(&svc, with_facets(note("Ship"), &["todo"])).await;
    let ship = set_property(&svc, &ship.id, "todo.due", date("2026-11-01"))
        .await
        .expect("set due");
    let mut rx = svc.hub.subscribe();
    // So the bump shows: timestamps are in milliseconds.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    let detached = detach(&svc, &ship.id, "todo").await.expect("detach");

    assert!(detached.facets.is_empty());
    assert!(
        detached
            .properties
            .keys()
            .all(|id| !id.starts_with("todo.")),
        "{:?}",
        detached.properties.keys()
    );
    assert!(stored_property_ids(&pool, &ship.id).await.is_empty());
    assert_eq!(facet_row_count(&pool).await, 0);
    assert!(millis(detached.updated_at) > millis(ship.updated_at));
    assert_eq!(only_upsert(&mut rx), detached);
}

#[tokio::test]
async fn detach_leaves_structure_properties_and_tags() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let urgent = create(&svc, tag("urgent")).await;
    let req = with_relations(
        with_facets(note("Ship"), &["todo"]),
        "tags",
        &[(&urgent.id, "Tag")],
    );
    let ship = create(&svc, req).await;
    // An id stored before I-79 that only looks like the namespace, in another case.
    store_legacy_value(
        &pool,
        &ship.id,
        "TODO.note",
        property_value::Value::Text("kept".to_string()),
    )
    .await;
    let ship = svc.load_entity(&ship.id).await.expect("load");

    let detached = detach(&svc, &ship.id, "todo").await.expect("detach");

    assert_eq!(
        relation_ids(&detached, "tags"),
        Some(vec![urgent.id.clone()])
    );
    assert_eq!(link_targets(&detached, "tags"), link_targets(&ship, "tags"));
    assert_eq!(
        stored_property_ids(&pool, &ship.id).await,
        vec!["TODO.note".to_string(), "tags".to_string()]
    );
}

#[tokio::test]
async fn detach_when_not_attached_is_a_no_op() {
    let svc = entity_service(memory_pool().await);
    let ship = create(&svc, note("Ship")).await;
    let mut rx = svc.hub.subscribe();

    let same = detach(&svc, &ship.id, "todo").await.expect("detach");

    assert_eq!(same, ship);
    assert!(rx.try_recv().is_err(), "no event");
}

#[tokio::test]
async fn detach_checks_the_entity_and_the_facet() {
    let svc = entity_service(memory_pool().await);
    let ship = create(&svc, note("Ship")).await;

    let missing = detach(&svc, "missing", "todo").await.expect_err("missing");
    assert_eq!(missing.code(), tonic::Code::NotFound);
    let unknown = detach(&svc, &ship.id, "chore").await.expect_err("unknown");
    assert_eq!(unknown.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn reattach_after_detach_writes_fresh_defaults() {
    let svc = entity_service(memory_pool().await);
    let ship = create(&svc, with_facets(note("Ship"), &["todo"])).await;
    set_property(&svc, &ship.id, "todo.status", select("done"))
        .await
        .expect("set status");
    detach(&svc, &ship.id, "todo").await.expect("detach");

    let again = attach(&svc, &ship.id, "todo").await.expect("attach");

    assert_eq!(select_value(&again, "todo.status").as_deref(), Some("open"));
}

#[tokio::test]
async fn set_facet_property_checks_its_select_options() {
    let svc = entity_service(memory_pool().await);
    let ship = create(&svc, with_facets(note("Ship"), &["todo"])).await;

    let err = set_property(&svc, &ship.id, "todo.status", select("someday"))
        .await
        .expect_err("not an option");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("open, done"), "{}", err.message());

    let wrong_kind = set_property(&svc, &ship.id, "todo.due", select("open"))
        .await
        .expect_err("due is a date");
    assert_eq!(wrong_kind.code(), tonic::Code::InvalidArgument);

    let saved = set_property(&svc, &ship.id, "todo.status", select("done"))
        .await
        .expect("set status");
    assert_eq!(select_value(&saved, "todo.status").as_deref(), Some("done"));
}

#[tokio::test]
async fn clearing_a_facet_property_without_the_facet_is_allowed() {
    let svc = entity_service(memory_pool().await);
    let ship = create(&svc, note("Ship")).await;

    let saved = set_property(&svc, &ship.id, "todo.status", None)
        .await
        .expect("clear");

    assert!(saved.properties.is_empty());
    assert!(saved.facets.is_empty());
}

#[tokio::test]
async fn delete_removes_facet_rows() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let ship = create(&svc, with_facets(note("Ship"), &["todo"])).await;
    let _kept = create(&svc, with_facets(note("Kept"), &["todo"])).await;
    assert_eq!(facet_row_count(&pool).await, 2);

    delete(&svc, &ship.id).await;

    assert_eq!(facet_row_count(&pool).await, 1);
}

#[test]
fn facets_load_in_registry_order_with_unknown_ones_last() {
    let mut facets = vec!["zeta".to_string(), "todo".to_string(), "alpha".to_string()];

    sort_facets(&mut facets);

    assert_eq!(facets, vec!["todo", "alpha", "zeta"]);
}
