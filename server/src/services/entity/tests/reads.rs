use calcifer::v1::{
    property_filter::Condition, CalendarItemKind, CountReferencesRequest, DateRange,
    GetEntitiesRequest, ListCalendarRequest, ListCalendarResponse, ListEntitiesResponse,
    MatchNamesRequest, PropertyFilter, PropertyValues,
};
use prost_types::FieldMask;

use super::*;

async fn list(svc: &EntityService, req: ListEntitiesRequest) -> ListEntitiesResponse {
    try_list(svc, req).await.expect("list")
}

async fn try_list(
    svc: &EntityService,
    req: ListEntitiesRequest,
) -> Result<ListEntitiesResponse, Status> {
    svc.list_entities(Request::new(req))
        .await
        .map(Response::into_inner)
}

fn names(entities: &[Entity]) -> Vec<&str> {
    entities.iter().map(|e| e.name.as_str()).collect()
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "fills a request's Option<FieldMask>"
)]
fn mask(paths: &[&str]) -> Option<FieldMask> {
    Some(FieldMask {
        paths: paths.iter().map(ToString::to_string).collect(),
    })
}

fn equals(property_id: &str, value: property_value::Value) -> PropertyFilter {
    PropertyFilter {
        property_id: property_id.to_string(),
        condition: Some(Condition::Equals(PropertyValue { value: Some(value) })),
    }
}

fn days(property_id: &str, start: &str, end: &str) -> PropertyFilter {
    PropertyFilter {
        property_id: property_id.to_string(),
        condition: Some(Condition::DateRange(DateRange {
            start: start.to_string(),
            end: end.to_string(),
        })),
    }
}

fn select_value(key: &str) -> property_value::Value {
    property_value::Value::Select(key.to_string())
}

fn date_value(day: &str) -> property_value::Value {
    property_value::Value::Date(day.to_string())
}

/// A to-do named `name`, with `status` and, if given, a due day.
async fn todo_with(svc: &EntityService, name: &str, status: &str, due: Option<&str>) -> Entity {
    let mut req = with_select(todo(name), "todo.status", status);
    if let Some(due) = due {
        req = with_value(req, "todo.due", date_value(due));
    }
    create(svc, req).await
}

/// The stored `value_key` of one property row, if the row exists.
async fn stored_value_key(
    pool: &SqlitePool,
    entity_id: &str,
    property_id: &str,
) -> Option<Option<String>> {
    sqlx::query_scalar("SELECT value_key FROM properties WHERE entity_id = ? AND property_id = ?")
        .bind(entity_id)
        .bind(property_id)
        .fetch_optional(pool)
        .await
        .expect("value_key")
}

#[tokio::test]
async fn every_property_write_keeps_its_value_key() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let task = todo_with(&svc, "Ship", "open", Some("2026-11-07")).await;
    assert_eq!(
        stored_value_key(&pool, &task.id, "todo.due").await,
        Some(Some("2026-11-07".into()))
    );
    // The facet's default.
    assert_eq!(
        stored_value_key(&pool, &task.id, "todo.priority").await,
        Some(Some("none".into()))
    );

    set_property(&svc, &task.id, "todo.due", date("2026-12-01"))
        .await
        .expect("set due");
    assert_eq!(
        stored_value_key(&pool, &task.id, "todo.due").await,
        Some(Some("2026-12-01".into()))
    );
    set_property(&svc, &task.id, "todo.due", None)
        .await
        .expect("clear due");
    assert_eq!(stored_value_key(&pool, &task.id, "todo.due").await, None);

    let other = create(&svc, note("Other")).await;
    attach(&svc, &other.id, "todo").await.expect("attach");
    assert_eq!(
        stored_value_key(&pool, &other.id, "todo.status").await,
        Some(Some("open".into()))
    );

    // Relation values have none.
    let urgent = create(&svc, tag("urgent")).await;
    add_target(&svc, &other.id, "tags", &urgent.id)
        .await
        .expect("tag");
    assert_eq!(stored_value_key(&pool, &other.id, "tags").await, Some(None));
}

#[tokio::test]
async fn list_filters_by_structure_facet_and_property() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    todo_with(&svc, "Early", "open", Some("2026-11-01")).await;
    todo_with(&svc, "Late", "open", Some("2026-11-20")).await;
    todo_with(&svc, "Undated", "open", None).await;
    todo_with(&svc, "Finished", "done", Some("2026-11-02")).await;
    create(&svc, note("Plain")).await;
    create(&svc, tag("plain")).await;

    let notes = list(
        &svc,
        ListEntitiesRequest {
            structure_type: "Note".into(),
            order_by: "name".into(),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        names(&notes.entities),
        ["Early", "Finished", "Late", "Plain", "Undated"]
    );

    let todos = |filters: Vec<PropertyFilter>| ListEntitiesRequest {
        facet: "todo".into(),
        filters,
        order_by: "name".into(),
        ..Default::default()
    };
    let all = list(&svc, todos(vec![])).await;
    assert_eq!(
        names(&all.entities),
        ["Early", "Finished", "Late", "Undated"]
    );
    let open = list(
        &svc,
        todos(vec![equals("todo.status", select_value("open"))]),
    )
    .await;
    assert_eq!(names(&open.entities), ["Early", "Late", "Undated"]);

    let either = PropertyFilter {
        property_id: "todo.status".into(),
        condition: Some(Condition::OneOf(PropertyValues {
            values: ["open", "done"]
                .iter()
                .map(|k| PropertyValue {
                    value: Some(select_value(k)),
                })
                .collect(),
        })),
    };
    assert_eq!(list(&svc, todos(vec![either])).await.entities.len(), 4);

    let this_week = list(
        &svc,
        todos(vec![
            equals("todo.status", select_value("open")),
            days("todo.due", "2026-11-01", "2026-11-07"),
        ]),
    )
    .await;
    assert_eq!(names(&this_week.entities), ["Early"]);
    let overdue = list(&svc, todos(vec![days("todo.due", "", "2026-11-02")])).await;
    assert_eq!(names(&overdue.entities), ["Early", "Finished"]);
    let dated = list(&svc, todos(vec![days("todo.due", "", "")])).await;
    assert_eq!(names(&dated.entities), ["Early", "Finished", "Late"]);

    // A value of another kind matches nothing.
    let wrong_kind = list(
        &svc,
        todos(vec![equals("todo.status", date_value("2026-11-01"))]),
    )
    .await;
    assert!(wrong_kind.entities.is_empty());
}

#[tokio::test]
async fn list_refuses_what_it_cannot_filter_or_order() {
    let pool = memory_pool().await;
    let svc = entity_service(pool);
    let tags_value = property_value::Value::Relations(EntityRefList::default());
    let bad = [
        ListEntitiesRequest {
            structure_type: "Nope".into(),
            ..Default::default()
        },
        ListEntitiesRequest {
            facet: "nope".into(),
            ..Default::default()
        },
        ListEntitiesRequest {
            filters: vec![equals("nope", select_value("x"))],
            ..Default::default()
        },
        ListEntitiesRequest {
            filters: vec![equals("tags", tags_value)],
            ..Default::default()
        },
        ListEntitiesRequest {
            filters: vec![days("todo.status", "2026-11-01", "")],
            ..Default::default()
        },
        ListEntitiesRequest {
            filters: vec![days("todo.due", "November 1", "")],
            ..Default::default()
        },
        ListEntitiesRequest {
            filters: vec![PropertyFilter {
                property_id: "todo.due".into(),
                condition: None,
            }],
            ..Default::default()
        },
        ListEntitiesRequest {
            order_by: "properties.content".into(),
            ..Default::default()
        },
        ListEntitiesRequest {
            page_size: -1,
            ..Default::default()
        },
        ListEntitiesRequest {
            page_token: "not a token".into(),
            ..Default::default()
        },
        ListEntitiesRequest {
            read_mask: mask(&["nope"]),
            ..Default::default()
        },
    ];
    for req in bad {
        let err = try_list(&svc, req.clone()).await.expect_err("refused");
        assert_eq!(err.code(), tonic::Code::InvalidArgument, "{req:?}");
    }
}

#[tokio::test]
async fn list_pages_do_not_shift_when_earlier_entities_change() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let mut made = HashMap::new();
    for name in ["Bravo", "Charlie", "Delta", "Echo", "Foxtrot"] {
        made.insert(name, create(&svc, note(name)).await);
    }
    let page = |token: String| ListEntitiesRequest {
        structure_type: "Note".into(),
        order_by: "name".into(),
        page_size: 2,
        page_token: token,
        ..Default::default()
    };

    let first = list(&svc, page(String::new())).await;
    assert_eq!(names(&first.entities), ["Bravo", "Charlie"]);
    assert!(!first.next_page_token.is_empty());

    // Before the page turns: one entity on the first page goes, and another
    // joins ahead of where the next page starts.
    delete(&svc, &made["Bravo"].id).await;
    create(&svc, note("Alpha")).await;

    let second = list(&svc, page(first.next_page_token)).await;
    assert_eq!(names(&second.entities), ["Delta", "Echo"]);
    let third = list(&svc, page(second.next_page_token)).await;
    assert_eq!(names(&third.entities), ["Foxtrot"]);
    assert!(third.next_page_token.is_empty());

    // No page_size: every match, with no token.
    let all = list(
        &svc,
        ListEntitiesRequest {
            structure_type: "Note".into(),
            order_by: "name desc".into(),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        names(&all.entities),
        ["Foxtrot", "Echo", "Delta", "Charlie", "Alpha"]
    );
    assert!(all.next_page_token.is_empty());

    // A token is for its own order only.
    let by_name = list(&svc, page(String::new())).await;
    let err = try_list(
        &svc,
        ListEntitiesRequest {
            order_by: "created_at".into(),
            page_size: 2,
            page_token: by_name.next_page_token,
            ..Default::default()
        },
    )
    .await
    .expect_err("another order's token");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn list_orders_by_a_property_with_missing_values_last() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    todo_with(&svc, "Later", "open", Some("2026-11-20")).await;
    todo_with(&svc, "Never", "open", None).await;
    todo_with(&svc, "Sooner", "open", Some("2026-11-01")).await;
    todo_with(&svc, "Someday", "open", None).await;

    for (order_by, expected) in [
        (
            "properties.todo.due",
            ["Sooner", "Later", "Never", "Someday"],
        ),
        (
            "properties.todo.due desc",
            ["Later", "Sooner", "Never", "Someday"],
        ),
    ] {
        let all = list(
            &svc,
            ListEntitiesRequest {
                facet: "todo".into(),
                order_by: order_by.into(),
                ..Default::default()
            },
        )
        .await;
        // Ties among the missing go by id, so compare them as a set.
        let got = names(&all.entities);
        assert_eq!(got[..2], expected[..2], "{order_by}");
        assert_eq!(
            sorted(got[2..].iter().map(ToString::to_string).collect()),
            ["Never", "Someday"]
        );

        // A page at a time, through the missing values too.
        let mut paged = vec![];
        let mut token = String::new();
        loop {
            let page = list(
                &svc,
                ListEntitiesRequest {
                    facet: "todo".into(),
                    order_by: order_by.into(),
                    page_size: 1,
                    page_token: token,
                    ..Default::default()
                },
            )
            .await;
            paged.extend(page.entities.into_iter().map(|e| e.name));
            if page.next_page_token.is_empty() {
                break;
            }
            token = page.next_page_token;
        }
        assert_eq!(paged, got, "{order_by}");
    }
}

#[tokio::test]
async fn list_defaults_to_most_recently_updated_first() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let a = create(&svc, note("A")).await;
    let b = create(&svc, note("B")).await;
    for (id, at) in [(&a.id, 2_000), (&b.id, 1_000)] {
        sqlx::query("UPDATE entities SET updated_at = ? WHERE id = ?")
            .bind(at)
            .bind(id)
            .execute(&pool)
            .await
            .expect("pin updated_at");
    }
    let first = list(
        &svc,
        ListEntitiesRequest {
            page_size: 1,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(names(&first.entities), ["A"]);
    let second = list(
        &svc,
        ListEntitiesRequest {
            page_size: 1,
            page_token: first.next_page_token,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(names(&second.entities), ["B"]);
    assert!(second.next_page_token.is_empty());
}

#[tokio::test]
async fn a_read_mask_returns_only_what_it_names() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let urgent = create(&svc, tag("urgent")).await;
    let task = create(
        &svc,
        with_relations(todo("Ship"), "tags", &[(&urgent.id, "Tag")]),
    )
    .await;
    put_content(&pool, &task.id, &[date_chip("2026-11-07")]).await;
    let full = svc
        .get_entity(Request::new(GetEntityRequest {
            id: task.id.clone(),
        }))
        .await
        .expect("get")
        .into_inner()
        .entity
        .expect("entity");

    let svc = &svc;
    let get = |paths: &[&str]| GetEntitiesRequest {
        ids: vec![task.id.clone()],
        read_mask: mask(paths),
    };
    let got = |req| async move {
        svc.get_entities(Request::new(req))
            .await
            .expect("get entities")
            .into_inner()
            .entities
            .remove(0)
    };

    let named = got(get(&["name"])).await;
    assert_eq!(
        named,
        Entity {
            id: task.id.clone(),
            structure_type: "Note".into(),
            name: "Ship".into(),
            ..Default::default()
        }
    );

    let due = got(get(&["properties.todo.status", "facets", "updated_at"])).await;
    assert_eq!(due.properties.keys().collect::<Vec<_>>(), ["todo.status"]);
    assert_eq!(due.facets, ["todo"]);
    assert_eq!(due.updated_at, full.updated_at);
    assert!(due.name.is_empty() && due.links.is_empty() && due.created_at.is_none());

    let graph = got(get(&["links", "referenced_dates"])).await;
    assert_eq!(graph.links, full.links);
    assert_eq!(graph.referenced_dates, ["2026-11-07"]);
    assert!(graph.properties.is_empty());

    assert_eq!(got(get(&[])).await, full);
}

#[tokio::test]
async fn get_entities_keeps_the_order_asked_for() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let a = create(&svc, note("A")).await;
    let b = create(&svc, note("B")).await;
    let c = create(&svc, tag("c")).await;
    let got = svc
        .get_entities(Request::new(GetEntitiesRequest {
            ids: vec![
                c.id.clone(),
                "missing".into(),
                a.id.clone(),
                c.id.clone(),
                b.id.clone(),
            ],
            read_mask: None,
        }))
        .await
        .expect("get entities")
        .into_inner()
        .entities;
    assert_eq!(got, [c, a, b]);
}

#[tokio::test]
async fn count_references_counts_distinct_sources() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let urgent = create(&svc, tag("urgent")).await;
    let quiet = create(&svc, tag("quiet")).await;
    // Tagged and mentioned: one source, two links.
    let ship = create(
        &svc,
        with_relations(note("Ship"), "tags", &[(&urgent.id, "Tag")]),
    )
    .await;
    put_content(&pool, &ship.id, &[mention("hashtag", &urgent)]).await;
    let plants = create(
        &svc,
        with_relations(note("Plants"), "tags", &[(&urgent.id, "Tag")]),
    )
    .await;
    // A link to itself isn't counted.
    put_content(&pool, &plants.id, &[mention("mention", &plants)]).await;

    let counts = svc
        .count_references(Request::new(CountReferencesRequest {
            entity_ids: vec![
                urgent.id.clone(),
                quiet.id.clone(),
                plants.id.clone(),
                "missing".into(),
            ],
        }))
        .await
        .expect("count")
        .into_inner()
        .counts;
    assert_eq!(
        counts,
        HashMap::from([
            (urgent.id.clone(), 2),
            (quiet.id.clone(), 0),
            (plants.id.clone(), 0),
            ("missing".to_string(), 0),
        ])
    );
}

#[tokio::test]
async fn match_names_matches_a_folded_prefix() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    create(&svc, note("Mercury Seven")).await;
    create(&svc, note("mercury")).await;
    create(&svc, note("Mars")).await;
    create(&svc, note("Королёв")).await;
    create(&svc, tag("mercury.program")).await;

    let svc = &svc;
    let matched = |query: &str, structure_types: &[&str], limit: u32| {
        let req = MatchNamesRequest {
            query: query.into(),
            structure_types: structure_types.iter().map(ToString::to_string).collect(),
            limit,
            read_mask: mask(&["name"]),
        };
        async move {
            svc.match_names(Request::new(req)).await.map(|r| {
                r.into_inner()
                    .entities
                    .into_iter()
                    .map(|e| e.name)
                    .collect::<Vec<_>>()
            })
        }
    };

    // The exact match first, then folded-name order.
    assert_eq!(
        matched(" MERC", &[], 0).await.expect("match"),
        ["mercury", "Mercury Seven", "mercury.program"]
    );
    assert_eq!(
        matched("mercury", &["Note"], 0).await.expect("match"),
        ["mercury", "Mercury Seven"]
    );
    assert_eq!(matched("КОРОЛ", &[], 0).await.expect("match"), ["Королёв"]);
    assert_eq!(matched("m", &[], 2).await.expect("match").len(), 2);
    assert_eq!(matched("", &["Note"], 0).await.expect("match").len(), 4);
    // A prefix of the name, not a word in it.
    assert!(matched("seven", &[], 0).await.expect("match").is_empty());

    let err = matched("m", &["Nope"], 0)
        .await
        .expect_err("unknown structure");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

async fn calendar(
    svc: &EntityService,
    start: &str,
    end: &str,
) -> Result<ListCalendarResponse, Status> {
    svc.list_calendar(Request::new(ListCalendarRequest {
        start: start.into(),
        end: end.into(),
        read_mask: None,
    }))
    .await
    .map(Response::into_inner)
}

/// (day, kind, entity id, property id) of each item, leaving out `Created`.
fn dated_items(res: &ListCalendarResponse) -> Vec<(String, CalendarItemKind, String, String)> {
    res.items
        .iter()
        .filter(|i| i.kind() != CalendarItemKind::Created)
        .map(|i| {
            (
                i.day.clone(),
                i.kind(),
                i.entity_id.clone(),
                i.property_id.clone(),
            )
        })
        .collect()
}

#[tokio::test]
async fn list_calendar_reads_flagged_properties_chips_and_created_days() {
    let pool = memory_pool().await;
    let svc = entity_service(pool.clone());
    let due = todo_with(&svc, "Ship", "open", Some("2026-11-03")).await;
    todo_with(&svc, "Far", "open", Some("2027-01-01")).await;
    let day = daily_note(&svc, "2026-11-05").await;
    let noted = create(&svc, note("Noted")).await;
    put_content(
        &pool,
        &noted.id,
        &[date_chip("2026-11-04"), date_chip("2026-12-25")],
    )
    .await;

    let week = calendar(&svc, "2026-11-01", "2026-11-07")
        .await
        .expect("calendar");
    let property = CalendarItemKind::Property;
    let reference = CalendarItemKind::Reference;
    assert_eq!(
        dated_items(&week),
        [
            (
                "2026-11-03".into(),
                property,
                due.id.clone(),
                "todo.due".into()
            ),
            (
                "2026-11-04".into(),
                reference,
                noted.id.clone(),
                String::new()
            ),
            ("2026-11-05".into(), property, day.id.clone(), "date".into()),
        ]
    );
    // Each item's entity comes once.
    let item_ids: std::collections::BTreeSet<&str> =
        week.items.iter().map(|i| i.entity_id.as_str()).collect();
    let entity_ids: Vec<&str> = week.entities.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(entity_ids, item_ids.into_iter().collect::<Vec<_>>());

    // Every entity was created today, on its local day.
    let today = chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    let created = calendar(&svc, &today, &today).await.expect("today");
    let created_ids: Vec<&str> = created
        .items
        .iter()
        .filter(|i| i.kind() == CalendarItemKind::Created)
        .map(|i| i.entity_id.as_str())
        .collect();
    assert_eq!(created_ids.len(), 4);
    assert!(created_ids.contains(&noted.id.as_str()));

    for (start, end) in [
        ("2026-11-07", "2026-11-01"),
        ("", "2026-11-01"),
        ("2026-11-01", "Nov 7"),
    ] {
        let err = calendar(&svc, start, end).await.expect_err("bad range");
        assert_eq!(err.code(), tonic::Code::InvalidArgument, "{start}..{end}");
    }
}
