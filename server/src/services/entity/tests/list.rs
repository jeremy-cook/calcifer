use super::*;

#[tokio::test]
async fn load_entities_matches_load_entity() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let urgent = create(&svc, tag("urgent")).await;
    let home = create(&svc, tag("home")).await;
    let other = create(&svc, note("Other")).await;
    let plants = create(
        &svc,
        with_relations(
            todo("Water plants"),
            "tags",
            &[(&urgent.id, "Tag"), (&home.id, "Tag")],
        ),
    )
    .await;
    let _bare = create(&svc, todo("Bare")).await;
    let ship = create(&svc, with_facets(note("Ship"), &["todo"])).await;
    let ideas = create(&svc, note("Ideas")).await;
    let day = daily_note(&svc, "2026-06-15").await;
    // Several links and dates per entity, dates written out of order, so the
    // ordering inside an entity is exercised.
    put_content(
        &pool,
        &ideas.id,
        &[
            mention("mention", &plants),
            mention("hashtag", &urgent),
            mention("mention", &other),
            date_chip("2026-09-01"),
            date_chip("2026-01-02"),
        ],
    )
    .await;
    put_content(
        &pool,
        &plants.id,
        &[mention("mention", &ideas), date_chip("2026-07-04")],
    )
    .await;
    put_content(
        &pool,
        &day.id,
        &[
            mention("hashtag", &home),
            date_chip("2026-12-25"),
            date_chip("2026-03-03"),
        ],
    )
    .await;

    for filter in [None, Some("Note"), Some("Tag"), Some("DailyNote")] {
        let mut expected = Vec::new();
        for id in listed_ids(&pool, filter).await {
            expected.push(load_entity(&pool, &id).await.expect("load"));
        }
        let batched = load_entities(&pool, filter).await.expect("load entities");
        assert_eq!(batched, expected, "filter {filter:?}");
    }
    let all = load_entities(&pool, None).await.expect("load entities");
    assert_eq!(all.len(), 8);
    let batched_ship = all.iter().find(|e| e.id == ship.id).expect("ship listed");
    assert_eq!(batched_ship.facets, vec!["todo".to_string()]);

    // By id: only the named entities that exist, each as `load_entity` has it.
    let ids = [
        ideas.id.clone(),
        "missing".to_string(),
        plants.id.clone(),
        day.id.clone(),
    ];
    let by_id = load_entities_by_id(&pool, &ids).await.expect("load by id");
    let mut expected: Vec<&Entity> = all
        .iter()
        .filter(|e| [&ideas.id, &plants.id, &day.id].contains(&&e.id))
        .collect();
    expected.sort_by_key(|e| e.id.clone());
    let mut got: Vec<&Entity> = by_id.iter().collect();
    got.sort_by_key(|e| e.id.clone());
    assert_eq!(got, expected);
    assert!(load_entities_by_id(&pool, &[])
        .await
        .expect("empty")
        .is_empty());

    let ideas = all.iter().find(|e| e.id == ideas.id).expect("ideas");
    assert_eq!(ideas.links.len(), 3);
    assert_eq!(ideas.referenced_dates, ["2026-01-02", "2026-09-01"]);
}

// I-48: entities written in the same millisecond come back in id order.
#[tokio::test]
async fn list_ties_are_broken_by_id() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let mut ids = Vec::new();
    for name in ["C", "A", "B", "D"] {
        ids.push(create(&svc, note(name)).await.id);
    }
    let newest = create(&svc, tag("Newest")).await;
    sqlx::query("UPDATE entities SET updated_at = 1000 WHERE id != ?")
        .bind(&newest.id)
        .execute(&pool)
        .await
        .expect("tie updated_at");
    sqlx::query("UPDATE entities SET updated_at = 2000 WHERE id = ?")
        .bind(&newest.id)
        .execute(&pool)
        .await
        .expect("newest");
    ids.sort();

    let listed = |structure_type: &str| {
        svc.list_entities(Request::new(ListEntitiesRequest {
            structure_type: structure_type.to_string(),
        }))
    };
    let all: Vec<String> = listed("")
        .await
        .expect("list")
        .into_inner()
        .entities
        .into_iter()
        .map(|e| e.id)
        .collect();
    let mut expected = vec![newest.id.clone()];
    expected.extend(ids.iter().cloned());
    assert_eq!(all, expected);

    let notes: Vec<String> = listed("Note")
        .await
        .expect("list notes")
        .into_inner()
        .entities
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert_eq!(notes, ids);

    let by_id: Vec<String> = load_entities_by_id(&pool, &ids)
        .await
        .expect("by id")
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert_eq!(by_id, ids);
}

#[tokio::test]
async fn list_backlinks_excludes_self_links() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let target = create(&svc, note("Target")).await;
    let source = create(&svc, note("Source")).await;
    put_content(&pool, &target.id, &[mention("mention", &target)]).await;
    put_content(&pool, &source.id, &[mention("mention", &target)]).await;

    let backlinks = list_backlinks_of(&svc, &target.id)
        .await
        .expect("backlinks");

    let ids: Vec<&str> = backlinks
        .iter()
        .map(|b| b.source.as_ref().expect("source").id.as_str())
        .collect();
    assert_eq!(ids, [source.id.as_str()]);
}

#[tokio::test]
async fn list_backlinks_reports_property_and_time_newest_first() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let target = create(&svc, tag("target")).await;
    let source = create(&svc, note("Source")).await;
    put_content(&pool, &source.id, &[mention("mention", &target)]).await;
    let chore = create(
        &svc,
        with_relations(todo("Chore"), "tags", &[(&target.id, "Tag")]),
    )
    .await;
    // The chore also mentions the target in its content: a second row.
    put_content(&pool, &chore.id, &[mention("mention", &target)]).await;
    set_link_created_at(&pool, &source.id, "content", 1_000).await;
    set_link_created_at(&pool, &chore.id, "tags", 2_000).await;
    set_link_created_at(&pool, &chore.id, "content", 3_000).await;

    let backlinks = list_backlinks_of(&svc, &target.id)
        .await
        .expect("backlinks");

    assert_eq!(
        backlink_rows(&backlinks),
        [
            (chore.id.clone(), "content".to_string(), 3_000),
            (chore.id.clone(), "tags".to_string(), 2_000),
            (source.id.clone(), "content".to_string(), 1_000),
        ]
    );
    // Each source is the full entity, as Get returns it.
    let loaded_source = load_entity(&pool, &source.id).await.expect("load source");
    assert_eq!(backlinks[2].source.as_ref(), Some(&loaded_source));

    // Equal times: the lower source id first.
    set_link_created_at(&pool, &source.id, "content", 5_000).await;
    set_link_created_at(&pool, &chore.id, "content", 5_000).await;
    set_link_created_at(&pool, &chore.id, "tags", 5_000).await;
    let backlinks = list_backlinks_of(&svc, &target.id)
        .await
        .expect("backlinks");
    let first = backlink_rows(&backlinks)[0].0.clone();
    assert_eq!(first, source.id.clone().min(chore.id.clone()));
}

#[tokio::test]
async fn list_backlinks_of_missing_entity_is_not_found() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());

    let err = list_backlinks_of(&svc, "missing")
        .await
        .expect_err("unknown entity");

    assert_eq!(err.code(), tonic::Code::NotFound);
}
