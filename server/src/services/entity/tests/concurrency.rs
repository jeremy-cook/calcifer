use super::*;

// I-15's done-when: the agent sets a to-do's status while the browser renames
// it; both edits survive.
#[tokio::test]
async fn rename_concurrent_with_set_property_keeps_both() {
    let svc = entity_service(memory_pool().await);
    let snapshot = create(&svc, todo("Water plants")).await;

    set_property(&svc, &snapshot.id, "todo.status", select("done"))
        .await
        .expect("agent sets status");
    rename(&svc, &snapshot.id, "Water the plants")
        .await
        .expect("browser renames");

    let stored = svc.load_entity(&snapshot.id).await.expect("load");
    assert_eq!(stored.name, "Water the plants");
    assert_eq!(
        select_value(&stored, "todo.status").as_deref(),
        Some("done")
    );
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
