use super::*;

#[tokio::test]
async fn watch_starts_with_a_snapshot_of_existing_entities() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let a = create(&svc, note("A")).await;
    let b = create(&svc, tag("b")).await;

    // No writes follow: the snapshot must come without waiting for an event.
    let mut stream = open_watch(&svc).await;
    let first = next_event(&mut stream).await;

    assert_eq!(
        snapshot_ids(&first),
        sorted(vec![a.id.clone(), b.id.clone()])
    );
    assert_eq!(first.revision, 2);
    assert_eq!(first.revision, svc.hub.current_revision());
    let Some(watch_entities_response::Event::Snapshot(snapshot)) = first.event else {
        unreachable!()
    };
    let snapshot_a = snapshot.entities.iter().find(|e| e.id == a.id);
    assert_eq!(snapshot_a, Some(&a));
}

#[tokio::test]
async fn watch_events_after_snapshot_have_increasing_revisions() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let mut stream = open_watch(&svc).await;
    let snapshot = next_event(&mut stream).await;
    assert!(snapshot_ids(&snapshot).is_empty());

    let a = create(&svc, note("A")).await;
    let b = create(&svc, todo("B")).await;
    rename(&svc, &a.id, "A2").await.expect("rename");
    svc.delete_entity(Request::new(DeleteEntityRequest { id: b.id.clone() }))
        .await
        .expect("delete");

    let mut revisions = vec![snapshot.revision];
    let mut kinds = vec![];
    for _ in 0..4 {
        let event = next_event(&mut stream).await;
        revisions.push(event.revision);
        kinds.push(match event.event {
            Some(watch_entities_response::Event::Upserted(e)) => format!("upserted {}", e.name),
            Some(watch_entities_response::Event::DeletedId(id)) => format!("deleted {id}"),
            other => panic!("unexpected event {other:?}"),
        });
    }
    assert_eq!(
        kinds,
        [
            "upserted A".to_string(),
            "upserted B".to_string(),
            "upserted A2".to_string(),
            format!("deleted {}", b.id),
        ]
    );
    assert!(
        revisions.windows(2).all(|w| w[0] < w[1]),
        "revisions increase: {revisions:?}"
    );
}

#[tokio::test]
async fn watch_does_not_repeat_events_covered_by_the_snapshot() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    // Subscribe, then publish before the stream takes its snapshot: the
    // window between `watch_entities`'s subscribe and its revision read.
    let rx = svc.hub.subscribe();
    let a = create(&svc, note("A")).await;
    let mut stream = watch_stream(pool.clone(), svc.hub.clone(), rx);

    let snapshot = next_event(&mut stream).await;
    assert_eq!(snapshot_ids(&snapshot), vec![a.id.clone()]);
    assert_eq!(snapshot.revision, 1);

    // A's upsert is still queued on the receiver; the stream must skip it.
    let b = create(&svc, note("B")).await;
    let next = next_event(&mut stream).await;
    assert_eq!(upserted_id(&next), b.id);
    assert_eq!(next.revision, 2);
}

#[tokio::test]
async fn watch_resyncs_with_a_snapshot_after_lag() {
    let pool = memory_pool().await;
    let svc = EntityService::new(pool, WatchHub::with_capacity(2), EmbedHandle::disabled());
    let mut stream = open_watch(&svc).await;
    assert!(snapshot_ids(&next_event(&mut stream).await).is_empty());

    // Five events into a channel of two, without reading: the stream lags.
    let mut ids = vec![];
    for i in 0..5 {
        ids.push(create(&svc, note(&format!("N{i}"))).await.id);
    }

    let resync = next_event(&mut stream).await;
    assert_eq!(snapshot_ids(&resync), sorted(ids));
    assert_eq!(resync.revision, 5);

    // The two events still in the channel are covered by the resync: the
    // stream reads and skips them, sending nothing.
    let quiet = tokio::time::timeout(std::time::Duration::from_millis(50), stream.next()).await;
    assert!(quiet.is_err(), "expected no event, got {quiet:?}");

    // The next event is the next write.
    let after = create(&svc, note("After")).await;
    let next = next_event(&mut stream).await;
    assert_eq!(upserted_id(&next), after.id);
    assert_eq!(next.revision, 6);
}
