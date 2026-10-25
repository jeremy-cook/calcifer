use super::*;

#[tokio::test]
async fn resolve_by_date_gets_or_creates() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let mut rx = svc.hub.subscribe();

    let first = resolve(&svc, "", by_date("2026-06-13"), true)
        .await
        .expect("create");
    assert!(first.created);
    let entity = first.entity.expect("entity");
    assert_eq!(entity.structure_type, "DailyNote");
    assert_eq!(entity.name, "June 13, 2026");
    assert_eq!(
        stored_date_key(&pool, &entity.id).await.as_deref(),
        Some("2026-06-13")
    );
    // No stored value for its rich-text `content` (I-22).
    assert!(!entity.properties.contains_key("content"));
    assert_eq!(upserted_ids(&mut rx), std::slice::from_ref(&entity.id));

    // Again, with the structure named: the same note, nothing published.
    let again = resolve(&svc, "DailyNote", by_date("2026-06-13"), true)
        .await
        .expect("get");
    assert!(!again.created);
    assert_eq!(again.entity.expect("entity").id, entity.id);
    assert!(upserted_ids(&mut rx).is_empty());
}

#[tokio::test]
async fn resolve_by_date_without_create_is_not_found() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());

    let err = resolve(&svc, "", by_date("2026-06-13"), false)
        .await
        .expect_err("no note for that day");

    assert_eq!(err.code(), tonic::Code::NotFound);
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entities")
        .fetch_one(&pool)
        .await
        .expect("count entities");
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn resolve_rejects_malformed_requests() {
    let svc = entity_service(memory_pool().await);

    for (structure_type, key) in [
        ("Note", by_date("2026-06-13")),
        ("", by_date("2026-6-13")),
        ("", by_date("June 13")),
        ("", by_date("")),
        ("", by_name("")),
        ("Note", by_name("  ")),
        ("", by_name("Orphan")),
    ] {
        let err = resolve(&svc, structure_type, key.clone(), true)
            .await
            .expect_err("bad request");
        assert_eq!(err.code(), tonic::Code::InvalidArgument, "{key:?}");
    }
    let err = svc
        .resolve_entity(Request::new(ResolveEntityRequest {
            structure_type: "Note".to_string(),
            key: None,
            create_if_missing: true,
        }))
        .await
        .expect_err("no key");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn resolve_by_name_gets_or_creates() {
    let svc = entity_service(memory_pool().await);
    let mut rx = svc.hub.subscribe();

    let first = resolve(&svc, "Note", by_name("  Water plants "), true)
        .await
        .expect("create");
    assert!(first.created);
    let entity = first.entity.expect("entity");
    assert_eq!(entity.structure_type, "Note");
    assert_eq!(entity.name, "Water plants");
    assert_eq!(upserted_ids(&mut rx), std::slice::from_ref(&entity.id));

    // Case-insensitive, and scoped to the structure.
    let again = resolve(&svc, "Note", by_name("WATER PLANTS"), true)
        .await
        .expect("get");
    assert!(!again.created);
    assert_eq!(again.entity.expect("entity").id, entity.id);
    assert!(upserted_ids(&mut rx).is_empty());

    let tag = resolve(&svc, "Tag", by_name("Water plants"), true)
        .await
        .expect("a Tag of the same name");
    assert!(tag.created);
    assert_ne!(tag.entity.expect("entity").id, entity.id);
}

// I-24, D2: with two same-named Notes, the oldest wins, then the lower id.
#[tokio::test]
async fn resolve_by_name_with_duplicates_returns_the_oldest() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let a = create(&svc, note("Same")).await;
    let b = create(&svc, note("same")).await;
    let (low, high) = if a.id < b.id { (&a, &b) } else { (&b, &a) };
    let set_created_at = |id: String, at: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query("UPDATE entities SET created_at = ? WHERE id = ?")
                .bind(at)
                .bind(id)
                .execute(&pool)
                .await
                .expect("set created_at");
        }
    };
    let resolved_id = || async {
        resolve(&svc, "Note", by_name("SAME"), false)
            .await
            .expect("resolve")
            .entity
            .expect("entity")
            .id
    };

    // The higher id is older, so it wins over the lower one.
    set_created_at(high.id.clone(), 1_000).await;
    set_created_at(low.id.clone(), 2_000).await;
    for _ in 0..3 {
        assert_eq!(resolved_id().await, high.id);
    }

    // Same created_at: the lower id breaks the tie.
    set_created_at(high.id.clone(), 2_000).await;
    for _ in 0..3 {
        assert_eq!(resolved_id().await, low.id);
    }
}

// A Tag is found by its folded name key, so any casing finds it.
#[tokio::test]
async fn resolve_tag_by_name_ignores_case() {
    let svc = entity_service(memory_pool().await);
    let reading = create(&svc, tag("Reading")).await;
    let cafe = create(&svc, tag("Café")).await;

    for (name, want) in [
        ("reading", &reading),
        (" READING ", &reading),
        ("café", &cafe),
        ("CAFÉ", &cafe),
    ] {
        let found = resolve(&svc, "Tag", by_name(name), true)
            .await
            .expect("resolve");
        assert!(!found.created, "{name:?}");
        assert_eq!(found.entity.expect("entity").id, want.id, "{name:?}");
    }
}

#[tokio::test]
async fn resolve_by_name_without_create_is_not_found() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    create(&svc, tag("ml")).await;

    let err = resolve(&svc, "Note", by_name("ml"), false)
        .await
        .expect_err("no Note named ml");

    assert_eq!(err.code(), tonic::Code::NotFound);
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entities")
        .fetch_one(&pool)
        .await
        .expect("count entities");
    assert_eq!(rows, 1);
}

// I-46: DailyNote isn't creatable, so Resolve by name can only look one up.
#[tokio::test]
async fn resolve_by_name_of_a_missing_daily_note_is_failed_precondition() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());

    let err = resolve(&svc, "DailyNote", by_name("Someday"), true)
        .await
        .expect_err("DailyNote isn't creatable");
    assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    assert!(err.message().contains("by date"), "{}", err.message());
    assert_eq!(entity_count(&pool).await, 0);

    let err = resolve(&svc, "DailyNote", by_name("Someday"), false)
        .await
        .expect_err("no such DailyNote");
    assert_eq!(err.code(), tonic::Code::NotFound);

    // A lookup of an existing one still works.
    let day = daily_note(&svc, "2026-06-13").await;
    let found = resolve(&svc, "DailyNote", by_name("june 13, 2026"), true)
        .await
        .expect("lookup");
    assert!(!found.created);
    assert_eq!(found.entity.expect("entity").id, day.id);
}
