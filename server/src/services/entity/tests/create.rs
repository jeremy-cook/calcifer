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

#[tokio::test]
async fn create_by_intent_mints_id_defaults_and_name() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let mut rx = svc.hub.subscribe();

    let first = create(&svc, with_facets(untitled("Note"), &["todo"])).await;
    let second = create(&svc, with_facets(untitled("Note"), &["todo"])).await;

    assert!(uuid::Uuid::parse_str(&first.id).is_ok(), "{}", first.id);
    assert_ne!(first.id, second.id);
    assert_eq!(first.structure_type, "Note");
    // Note names aren't unique, so the default may repeat.
    assert_eq!(first.name, "Untitled Note");
    assert_eq!(second.name, "Untitled Note");
    assert_eq!(select_value(&first, "todo.status").as_deref(), Some("open"));
    assert_eq!(
        select_value(&first, "todo.priority").as_deref(),
        Some("none")
    );
    // No stored value for the rich-text `content` (I-22), here or in the DB.
    assert!(!first.properties.contains_key("content"));
    assert!(stored_property_ids(&pool, &first.id)
        .await
        .iter()
        .all(|id| id != "content"));
    // Server-owned fields: no links or dates until content is saved (ADR 3).
    assert!(first.links.is_empty());
    assert!(first.referenced_dates.is_empty());
    assert_eq!(millis(first.created_at), millis(first.updated_at));
    assert_eq!(fts_name(&pool, &first.id).await, "Untitled Note");
    assert_eq!(upserted_ids(&mut rx), [first.id.clone(), second.id.clone()]);

    // A given name is trimmed, and given properties win over the defaults.
    let named = create(
        &svc,
        with_select(todo("  Water plants "), "todo.priority", "high"),
    )
    .await;
    assert_eq!(named.name, "Water plants");
    assert_eq!(
        select_value(&named, "todo.priority").as_deref(),
        Some("high")
    );
    assert_eq!(select_value(&named, "todo.status").as_deref(), Some("open"));

    // A blank name is no name.
    let blank = create(&svc, note("  ")).await;
    assert_eq!(blank.name, "Untitled Note");
}

#[tokio::test]
async fn create_rejects_unknown_select_value() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());

    let err = try_create(
        &svc,
        with_select(todo("Water plants"), "todo.priority", "urgent"),
    )
    .await
    .expect_err("unknown priority should be rejected");

    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("urgent"), "{}", err.message());
    assert_eq!(entity_count(&pool).await, 0);
}

// I-24: a type the registry doesn't declare can't create, resolve or filter.
#[tokio::test]
async fn unknown_structure_types_are_rejected() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    create(&svc, note("Kept")).await;

    for structure_type in ["Custom", ""] {
        let err = try_create(&svc, untitled(structure_type))
            .await
            .expect_err("create of an unknown type");
        assert_eq!(
            err.code(),
            tonic::Code::InvalidArgument,
            "{structure_type:?}"
        );
    }
    let err = resolve(&svc, "Custom", by_name("Anything"), true)
        .await
        .expect_err("resolve of an unknown type");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(entity_count(&pool).await, 1);

    let err = svc
        .list_entities(Request::new(ListEntitiesRequest {
            structure_type: "Custom".to_string(),
        }))
        .await
        .expect_err("list of an unknown type");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    // An empty filter still lists every structure.
    let all = svc
        .list_entities(Request::new(ListEntitiesRequest {
            structure_type: String::new(),
        }))
        .await
        .expect("list all")
        .into_inner();
    assert_eq!(all.entities.len(), 1);
}

#[tokio::test]
async fn create_rejects_note_in_todo_tags() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let urgent = create(&svc, tag("urgent")).await;
    let other = create(&svc, note("Not a tag")).await;

    let err = try_create(
        &svc,
        with_relations(
            todo("Water plants"),
            "tags",
            &[(&urgent.id, "Tag"), (&other.id, "Note")],
        ),
    )
    .await
    .expect_err("a Note in tags should be rejected");

    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("tags"), "{}", err.message());
    assert_eq!(entity_count(&pool).await, 2);
}
