use super::*;

#[test]
fn new_entity_builds_defaults_and_lets_the_caller_win() {
    let entity = new_entity(
        "Note",
        Some("Water plants"),
        HashMap::from([(
            "todo.priority".to_string(),
            PropertyValue {
                value: Some(property_value::Value::Select("high".to_string())),
            },
        )]),
        &[structures::facet("todo").expect("todo facet")],
    );

    assert_eq!(entity.name, "Water plants");
    assert_eq!(
        select_value(&entity, "todo.status").as_deref(),
        Some("open")
    );
    assert_eq!(
        select_value(&entity, "todo.priority").as_deref(),
        Some("high")
    );
    // The defaults (status, priority) with the caller's priority laid over.
    assert_eq!(entity.properties.len(), 2);
    // Rich-text properties get no value (I-22).
    assert!(!entity.properties.contains_key("content"));
}

// A map entry can't omit its value, so a value with no case is the "missing
// value" a Property with no value used to be (I-28).
#[tokio::test]
async fn create_with_an_empty_property_value_is_invalid_argument() {
    let svc = entity_service(memory_pool().await);
    let mut req = note("Empty");
    req.properties
        .insert("mood".to_string(), PropertyValue { value: None });

    let err = try_create(&svc, req).await.expect_err("empty value");

    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[test]
fn new_entity_names_a_daily_note_for_its_date() {
    let entity = new_entity(
        "DailyNote",
        Some("Ignored"),
        date_properties("2026-06-13"),
        &[],
    );
    assert_eq!(entity.name, "June 13, 2026");
}

// I-26: DailyNote isn't creatable; Create points at Resolve, which still
// creates it by date.
#[tokio::test]
async fn create_of_a_daily_note_is_failed_precondition() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());

    let err = try_create(
        &svc,
        CreateEntityRequest {
            structure_type: "DailyNote".to_string(),
            name: None,
            properties: date_properties("2026-06-13"),
            facets: vec![],
        },
    )
    .await
    .expect_err("DailyNote isn't creatable");

    assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    assert!(err.message().contains("Resolve"), "{}", err.message());
    assert_eq!(entity_count(&pool).await, 0);

    let day = daily_note(&svc, "2026-06-13").await;
    assert_eq!(day.name, "June 13, 2026");
}

// I-52: a date value is a real day in canonical form (todo.due,
// DailyNote.date), on Create and SetProperty.
#[tokio::test]
async fn date_values_must_be_iso_days() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let task = create(&svc, todo("Task")).await;
    let day = daily_note(&svc, "2026-06-13").await;
    let targets = [(&task, "todo.due", todo("T")), (&day, "date", note("D"))];

    for bad in ["2026-13-45", "2026-2-3", "June 13", ""] {
        let value = property_value::Value::Date(bad.to_string());
        for (entity, property_id, req) in &targets {
            if entity.structure_type != "DailyNote" {
                let err = try_create(&svc, with_value(req.clone(), property_id, value.clone()))
                    .await
                    .expect_err("bad date on create");
                assert_eq!(err.code(), tonic::Code::InvalidArgument, "create {bad:?}");
            }
            let err = set_property(&svc, &entity.id, property_id, Some(value.clone()))
                .await
                .expect_err("bad date on set");
            assert_eq!(err.code(), tonic::Code::InvalidArgument, "set {bad:?}");
        }
    }
    assert_eq!(entity_count(&pool).await, 2);
    assert_eq!(
        stored_date_key(&pool, &day.id).await.as_deref(),
        Some("2026-06-13")
    );

    let good = property_value::Value::Date("2026-02-03".to_string());
    try_create(&svc, with_value(todo("T"), "todo.due", good.clone()))
        .await
        .expect("declared date on create");
    for (entity, property_id, _) in &targets {
        set_property(&svc, &entity.id, property_id, Some(good.clone()))
            .await
            .expect("good date on set");
    }
    assert_eq!(
        stored_date_key(&pool, &day.id).await.as_deref(),
        Some("2026-02-03")
    );
}

#[tokio::test]
async fn set_property_date_renames_daily_note() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let note = daily_note(&svc, "2026-06-13").await;

    let moved = set_property(&svc, &note.id, "date", date("2026-06-15"))
        .await
        .expect("move to a free day");

    assert_eq!(moved.name, "June 15, 2026");
    assert_eq!(fts_name(&pool, &note.id).await, "June 15, 2026");
    assert_eq!(
        stored_date_key(&pool, &note.id).await.as_deref(),
        Some("2026-06-15")
    );

    // Clearing the date keeps the name it had.
    let cleared = set_property(&svc, &note.id, "date", None)
        .await
        .expect("clear date");
    assert_eq!(cleared.name, "June 15, 2026");
    assert_eq!(stored_date_key(&pool, &note.id).await, None);
}

#[tokio::test]
async fn set_property_date_onto_existing_day_is_already_exists() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    daily_note(&svc, "2026-06-13").await;
    let second = daily_note(&svc, "2026-06-14").await;

    let err = set_property(&svc, &second.id, "date", date("2026-06-13"))
        .await
        .expect_err("day already has a note");

    assert_eq!(err.code(), tonic::Code::AlreadyExists);
    assert_eq!(err.message(), "a DailyNote for 2026-06-13 already exists");
    let stored = svc.load_entity(&second.id).await.expect("load");
    assert_eq!(stored.name, "June 14, 2026");
    assert_eq!(stored.properties, second.properties);
    assert_eq!(fts_name(&pool, &second.id).await, "June 14, 2026");
    assert_eq!(
        stored_date_key(&pool, &second.id).await.as_deref(),
        Some("2026-06-14")
    );
}

#[tokio::test]
async fn set_property_moves_a_daily_note_date_key() {
    let svc = entity_service(memory_pool().await);
    let first = daily_note(&svc, "2026-06-13").await;
    let second = daily_note(&svc, "2026-06-14").await;

    set_property(&svc, &first.id, "date", date("2026-06-15"))
        .await
        .expect("move to a free day");
    // The old day is free again; the new one is taken.
    let resolved = resolve(&svc, "", by_date("2026-06-13"), true)
        .await
        .expect("old day is free");
    assert!(resolved.created);
    let err = set_property(&svc, &second.id, "date", date("2026-06-15"))
        .await
        .expect_err("day already has a note");
    assert_eq!(err.code(), tonic::Code::AlreadyExists);
}

#[tokio::test]
async fn clearing_a_daily_note_date_removes_its_key_row() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let day = daily_note(&svc, "2026-06-13").await;

    set_property(&svc, &day.id, "date", None)
        .await
        .expect("clear date");

    assert_eq!(stored_date_key(&pool, &day.id).await, None);
    assert_eq!(key_row_count(&pool).await, 0);
    let resolved = resolve(&svc, "", by_date("2026-06-13"), true)
        .await
        .expect("the day is free");
    assert!(resolved.created);
}
