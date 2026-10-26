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

// I-36: two get-or-creates of one Tag race. The loser's insert fails the name
// key, and it returns the winner's Tag instead of ALREADY_EXISTS. A few rounds,
// because one race can miss the window.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn racing_resolves_of_one_tag_return_the_same_tag() {
    let scratch = ScratchDb::new().await;
    let svc = entity_service(scratch.pool.clone());

    for round in 0..5 {
        let (a, b) = tokio::join!(
            resolve(&svc, "Tag", by_name(&format!("urgent {round}")), true),
            resolve(&svc, "Tag", by_name(&format!("URGENT {round}")), true),
        );
        let (a, b) = (a.expect("first resolve"), b.expect("second resolve"));
        assert_eq!(
            [a.created, b.created].iter().filter(|c| **c).count(),
            1,
            "exactly one of them creates it"
        );
        assert_eq!(a.entity.expect("entity").id, b.entity.expect("entity").id);
    }
    scratch.pool.close().await;
}

// I-36 for the date key: two get-or-creates of one day's DailyNote race, and
// both return the one note.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn racing_resolves_of_one_day_return_the_same_daily_note() {
    let scratch = ScratchDb::new().await;
    let svc = entity_service(scratch.pool.clone());

    for day in 10..15 {
        let date = format!("2026-06-{day}");
        let (a, b) = tokio::join!(
            resolve(&svc, "", by_date(&date), true),
            resolve(&svc, "DailyNote", by_date(&date), true),
        );
        let (a, b) = (a.expect("first resolve"), b.expect("second resolve"));
        assert_eq!(
            [a.created, b.created].iter().filter(|c| **c).count(),
            1,
            "exactly one of them creates it"
        );
        assert_eq!(a.entity.expect("entity").id, b.entity.expect("entity").id);
    }
    scratch.pool.close().await;
}

// I-45: untitled Tags created at once each look up the first free name before
// they insert, so they can pick the same one. The loser looks again and takes
// the next free name, so every create succeeds.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn racing_untitled_tag_creates_get_distinct_names() {
    let scratch = ScratchDb::new().await;
    let svc = entity_service(scratch.pool.clone());

    let mut names = Vec::new();
    for _ in 0..5 {
        let (a, b, c) = tokio::join!(
            try_create(&svc, untitled("Tag")),
            try_create(&svc, untitled("Tag")),
            try_create(&svc, untitled("Tag")),
        );
        for created in [a, b, c] {
            names.push(created.expect("create").name);
        }
    }
    let distinct: std::collections::BTreeSet<&String> = names.iter().collect();
    assert_eq!(distinct.len(), names.len(), "{names:?}");
    scratch.pool.close().await;
}
