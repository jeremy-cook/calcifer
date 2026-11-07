use super::*;

/// The next event on the stream that the replica reads, skipping the change
/// feed's.
async fn next_replica(stream: &mut EventStream) -> WatchEntitiesResponse {
    loop {
        let event = next_event(stream).await;
        if !matches!(
            event.event,
            Some(
                watch_entities_response::Event::Changed(_)
                    | watch_entities_response::Event::Deleted(_)
            )
        ) {
            return event;
        }
    }
}

fn is_ready(event: &WatchEntitiesResponse) -> bool {
    matches!(event.event, Some(watch_entities_response::Event::Ready(_)))
}

/// The change feed's events waiting on `rx`, as `changed <name>` and
/// `deleted <structure> <id>`, skipping the replica's.
fn feed(rx: &mut broadcast::Receiver<WatchEntitiesResponse>) -> Vec<String> {
    let mut events = vec![];
    while let Ok(event) = rx.try_recv() {
        match event.event {
            Some(watch_entities_response::Event::Changed(c)) => {
                events.push(format!("changed {}", c.entity.expect("entity").name));
            }
            Some(watch_entities_response::Event::Deleted(d)) => {
                let r = d.r#ref.expect("ref");
                events.push(format!("deleted {} {}", r.structure_type, r.id));
            }
            _ => {}
        }
    }
    events
}

/// `changed <name>` for each entity, in id order: how a write publishes the
/// link targets it touched.
fn changed_in_id_order(entities: &[&Entity]) -> Vec<String> {
    let mut entities = entities.to_vec();
    entities.sort_by_key(|e| e.id.clone());
    entities
        .iter()
        .map(|e| format!("changed {}", e.name))
        .collect()
}

#[tokio::test]
async fn watch_starts_with_a_snapshot_of_existing_entities_then_ready() {
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
    // Each create published `upserted` and `changed`.
    assert_eq!(first.revision, 4);
    assert_eq!(first.revision, svc.hub.current_revision());
    let Some(watch_entities_response::Event::Snapshot(snapshot)) = first.event else {
        unreachable!()
    };
    let snapshot_a = snapshot.entities.iter().find(|e| e.id == a.id);
    assert_eq!(snapshot_a, Some(&a));

    let ready = next_event(&mut stream).await;
    assert!(is_ready(&ready), "{ready:?}");
    assert_eq!(ready.revision, first.revision);
}

#[tokio::test]
async fn watch_events_after_snapshot_have_increasing_revisions() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let mut stream = open_watch(&svc).await;
    let snapshot = next_event(&mut stream).await;
    assert!(snapshot_ids(&snapshot).is_empty());
    assert!(is_ready(&next_event(&mut stream).await));

    let a = create(&svc, note("A")).await;
    let b = create(&svc, todo("B")).await;
    rename(&svc, &a.id, "A2").await.expect("rename");
    svc.delete_entity(Request::new(DeleteEntityRequest { id: b.id.clone() }))
        .await
        .expect("delete");

    let mut revisions = vec![snapshot.revision];
    let mut kinds = vec![];
    for _ in 0..8 {
        let event = next_event(&mut stream).await;
        revisions.push(event.revision);
        kinds.push(match event.event {
            Some(watch_entities_response::Event::Upserted(e)) => format!("upserted {}", e.name),
            Some(watch_entities_response::Event::Changed(c)) => {
                format!("changed {}", c.entity.expect("entity").name)
            }
            Some(watch_entities_response::Event::DeletedId(id)) => format!("deleted_id {id}"),
            Some(watch_entities_response::Event::Deleted(d)) => {
                format!("deleted {}", d.r#ref.expect("ref").id)
            }
            other => panic!("unexpected event {other:?}"),
        });
    }
    assert_eq!(
        kinds,
        [
            "upserted A".to_string(),
            "changed A".to_string(),
            "upserted B".to_string(),
            "changed B".to_string(),
            "upserted A2".to_string(),
            "changed A2".to_string(),
            format!("deleted_id {}", b.id),
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
    assert_eq!(snapshot.revision, 2);
    let ready = next_event(&mut stream).await;
    assert!(is_ready(&ready));
    assert_eq!(ready.revision, 2);

    // A's events are still queued on the receiver; the stream must skip them.
    let b = create(&svc, note("B")).await;
    let next = next_event(&mut stream).await;
    assert_eq!(upserted_id(&next), b.id);
    assert_eq!(next.revision, 3);
}

#[tokio::test]
async fn watch_resyncs_with_a_snapshot_after_lag() {
    let pool = memory_pool().await;
    let svc = EntityService::new(pool, WatchHub::with_capacity(2), EmbedHandle::disabled());
    let mut stream = open_watch(&svc).await;
    assert!(snapshot_ids(&next_event(&mut stream).await).is_empty());
    assert!(is_ready(&next_event(&mut stream).await));

    // Ten events into a channel of two, without reading: the stream lags.
    let mut ids = vec![];
    for i in 0..5 {
        ids.push(create(&svc, note(&format!("N{i}"))).await.id);
    }

    let resync = next_event(&mut stream).await;
    assert_eq!(snapshot_ids(&resync), sorted(ids));
    assert_eq!(resync.revision, 10);
    let marker = next_event(&mut stream).await;
    assert!(
        matches!(
            marker.event,
            Some(watch_entities_response::Event::Resync(_))
        ),
        "{marker:?}"
    );
    assert_eq!(marker.revision, 10);

    // The two events still in the channel are covered by the resync: the
    // stream reads and skips them, sending nothing.
    let quiet = tokio::time::timeout(std::time::Duration::from_millis(50), stream.next()).await;
    assert!(quiet.is_err(), "expected no event, got {quiet:?}");

    // The next event is the next write.
    let after = create(&svc, note("After")).await;
    let next = next_replica(&mut stream).await;
    assert_eq!(upserted_id(&next), after.id);
    assert_eq!(next.revision, 11);
}

// ADR 9: a link added or removed changes both ends, so the target is
// published as it is now, after the entity written.
#[tokio::test]
async fn rich_text_links_publish_the_targets_gained_and_lost() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let apollo = create(&svc, note("Apollo 7")).await;
    let mercury = create(&svc, note("Mercury")).await;
    let gemini = create(&svc, note("Gemini")).await;
    let saturn = create(&svc, note("Saturn")).await;
    let rich = richtext_service(pool.clone(), svc.hub.clone());
    let put = |nodes: Vec<String>| {
        let doc = format!(
            r#"{{"type":"doc","content":[{{"type":"paragraph","content":[{}]}}]}}"#,
            nodes.join(",")
        );
        rich.put_rich_text(Request::new(PutRichTextRequest {
            entity_id: apollo.id.clone(),
            property_id: "content".into(),
            doc,
            expected_updated_at: None,
        }))
    };
    let mut rx = svc.hub.subscribe();

    put(vec![
        mention("mention", &mercury),
        mention("mention", &gemini),
    ])
    .await
    .expect("put");
    let mut expected = vec!["changed Apollo 7".to_string()];
    expected.extend(changed_in_id_order(&[&mercury, &gemini]));
    assert_eq!(feed(&mut rx), expected);

    // Mercury goes and Saturn comes; Gemini stays, so it isn't published.
    put(vec![
        mention("mention", &gemini),
        mention("mention", &saturn),
    ])
    .await
    .expect("put");
    let mut expected = vec!["changed Apollo 7".to_string()];
    expected.extend(changed_in_id_order(&[&mercury, &saturn]));
    assert_eq!(feed(&mut rx), expected);

    // An edit that keeps the links publishes the entity alone.
    put(vec![
        mention("mention", &saturn),
        mention("mention", &gemini),
    ])
    .await
    .expect("put");
    assert_eq!(feed(&mut rx), ["changed Apollo 7"]);
}

#[tokio::test]
async fn relation_edits_publish_their_targets() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let urgent = create(&svc, tag("urgent")).await;
    let home = create(&svc, tag("home")).await;
    let mut rx = svc.hub.subscribe();

    let ship = create(
        &svc,
        with_relations(note("Ship"), "tags", &[(&urgent.id, "Tag")]),
    )
    .await;
    assert_eq!(feed(&mut rx), ["changed Ship", "changed urgent"]);

    add_target(&svc, &ship.id, "tags", &home.id)
        .await
        .expect("add");
    assert_eq!(feed(&mut rx), ["changed Ship", "changed home"]);
    remove_target(&svc, &ship.id, "tags", &urgent.id)
        .await
        .expect("remove");
    assert_eq!(feed(&mut rx), ["changed Ship", "changed urgent"]);

    set_property(&svc, &ship.id, "tags", tags_value(&[(&urgent.id, "Tag")]))
        .await
        .expect("set");
    let mut expected = vec!["changed Ship".to_string()];
    expected.extend(changed_in_id_order(&[&urgent, &home]));
    assert_eq!(feed(&mut rx), expected);

    let plan = create(&svc, note("Plan")).await;
    feed(&mut rx);
    add_rel(&svc, &ship.id, &plan.id).await.expect("relate");
    assert_eq!(feed(&mut rx), ["changed Ship", "changed Plan"]);
    remove_rel(&svc, &ship.id, &plan.id)
        .await
        .expect("unrelate");
    assert_eq!(feed(&mut rx), ["changed Ship", "changed Plan"]);

    // A write that changes no link publishes the entity alone.
    rename(&svc, &ship.id, "Ship it").await.expect("rename");
    assert_eq!(feed(&mut rx), ["changed Ship it"]);
}

#[tokio::test]
async fn delete_publishes_its_structure_and_the_entities_it_linked() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let apollo = create(&svc, note("Apollo 7")).await;
    let mercury = create(&svc, note("Mercury")).await;
    let saturn = create(&svc, note("Saturn")).await;
    // Apollo 7 mentions Mercury, which mentions Saturn.
    put_content(&pool, &apollo.id, &[mention("mention", &mercury)]).await;
    put_content(&pool, &mercury.id, &[mention("mention", &saturn)]).await;
    let mut rx = svc.hub.subscribe();

    delete(&svc, &mercury.id).await;

    let mut expected = vec![format!("deleted Note {}", mercury.id)];
    expected.extend(changed_in_id_order(&[&apollo, &saturn]));
    assert_eq!(feed(&mut rx), expected);
    let apollo_now = svc.load_entity(&apollo.id).await.expect("load");
    assert!(apollo_now.links.is_empty(), "the swept link is gone");

    // An unknown id deletes nothing: the replica hears it as before, the feed
    // doesn't.
    let mut rx = svc.hub.subscribe();
    delete(&svc, "ghost").await;
    assert!(feed(&mut rx).is_empty());
}
