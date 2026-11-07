use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::{Local, NaiveDate, TimeZone};
use sqlx::{QueryBuilder, Row, Sqlite};
use tonic::Status;

use super::load::load_in_order;
use super::lookup::{known_facet, known_structure};
use super::mask::ReadMask;
use super::page::{scalar_property, Cursor, Order, OrderKey};
use super::value_key::value_key;
use super::EntityService;
use crate::dates::is_iso_day;
use crate::error::AppError;
use crate::structures::{self, fold_name, PropertyKind};
use calcifer::v1::{
    property_filter::Condition, CalendarItem, CalendarItemKind, DateRange, Entity,
    ListCalendarRequest, ListCalendarResponse, ListEntitiesRequest, ListEntitiesResponse,
    MatchNamesRequest, PropertyFilter,
};

/// MatchNames' limit when the request gives none, as Search's.
const DEFAULT_MATCH_LIMIT: u32 = 10;

/// A ListEntities property filter, checked and turned into value keys.
enum Filter {
    /// The property's `value_key` is one of these. Empty matches nothing.
    OneOf(String, Vec<String>),
    /// The property's day is in the range; a bound may be open.
    Days(String, Option<String>, Option<String>),
}

impl EntityService {
    /// ListEntities (ADR 9): the ids of one page, chosen and ordered in SQL,
    /// then hydrated with the mask.
    pub(super) async fn list_page(
        &self,
        req: ListEntitiesRequest,
    ) -> Result<ListEntitiesResponse, Status> {
        let ListEntitiesRequest {
            structure_type,
            facet,
            filters,
            order_by,
            page_size,
            page_token,
            read_mask,
        } = req;
        if !structure_type.is_empty() {
            known_structure(&structure_type)?;
        }
        if !facet.is_empty() {
            known_facet(&facet)?;
        }
        let filters = filters
            .into_iter()
            .map(parse_filter)
            .collect::<Result<Vec<_>, _>>()?;
        let order = Order::parse(&order_by)?;
        let after = order.cursor(&page_token)?;
        let page_size = usize::try_from(page_size)
            .map_err(|_| Status::invalid_argument("page_size can't be negative"))?;
        let mask = ReadMask::parse(read_mask.as_ref())?;
        if filters
            .iter()
            .any(|f| matches!(f, Filter::OneOf(_, keys) if keys.is_empty()))
        {
            return Ok(ListEntitiesResponse::default());
        }

        let key_column = match &order.key {
            OrderKey::UpdatedAt => "e.updated_at",
            OrderKey::CreatedAt => "e.created_at",
            OrderKey::Name => "e.name_key",
            OrderKey::Property(_) => "sp.value_key",
        };
        let mut sql: QueryBuilder<Sqlite> =
            QueryBuilder::new(format!("SELECT e.id, {key_column} FROM entities e"));
        if let OrderKey::Property(id) = &order.key {
            sql.push(" LEFT JOIN properties sp ON sp.entity_id = e.id AND sp.property_id = ")
                .push_bind(id.clone());
        }
        sql.push(" WHERE 1");
        if !structure_type.is_empty() {
            sql.push(" AND e.structure_type = ")
                .push_bind(structure_type);
        }
        if !facet.is_empty() {
            sql.push(" AND e.id IN (SELECT entity_id FROM entity_facets WHERE facet = ")
                .push_bind(facet)
                .push(")");
        }
        for filter in filters {
            sql.push(" AND e.id IN (SELECT entity_id FROM properties WHERE property_id = ");
            match filter {
                Filter::OneOf(property_id, keys) => {
                    sql.push_bind(property_id).push(" AND value_key IN (");
                    let mut keys_sql = sql.separated(", ");
                    for key in keys {
                        keys_sql.push_bind(key);
                    }
                    sql.push(")");
                }
                Filter::Days(property_id, start, end) => {
                    sql.push_bind(property_id)
                        .push(" AND value_key IS NOT NULL");
                    if let Some(start) = start {
                        sql.push(" AND value_key >= ").push_bind(start);
                    }
                    if let Some(end) = end {
                        sql.push(" AND value_key <= ").push_bind(end);
                    }
                }
            }
            sql.push(")");
        }
        // Past the cursor: a later key, or the same key and a higher id. An entity
        // without the ordering property sorts after every one with it.
        if let Some((key, id)) = after {
            let later = if order.descending { "<" } else { ">" };
            match key {
                Cursor::Missing => {
                    sql.push(format!(" AND {key_column} IS NULL AND e.id > "))
                        .push_bind(id);
                }
                key => {
                    sql.push(format!(" AND ({key_column} {later} "));
                    push_cursor_key(&mut sql, key.clone());
                    sql.push(format!(" OR ({key_column} = "));
                    push_cursor_key(&mut sql, key);
                    sql.push(" AND e.id > ").push_bind(id).push(")");
                    if matches!(order.key, OrderKey::Property(_)) {
                        sql.push(format!(" OR {key_column} IS NULL"));
                    }
                    sql.push(")");
                }
            }
        }
        let direction = if order.descending { " DESC" } else { "" };
        if matches!(order.key, OrderKey::Property(_)) {
            sql.push(format!(
                " ORDER BY {key_column} IS NULL, {key_column}{direction}, e.id"
            ));
        } else {
            sql.push(format!(" ORDER BY {key_column}{direction}, e.id"));
        }
        if page_size > 0 {
            // One more than the page, to know whether another follows.
            sql.push(" LIMIT ").push_bind(page_size as i64 + 1);
        }
        let rows = sql
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)?;

        let mut page: Vec<(String, Cursor)> = rows
            .iter()
            .map(|row| {
                let key = match order.key {
                    OrderKey::UpdatedAt | OrderKey::CreatedAt => Cursor::Millis(row.try_get(1)?),
                    OrderKey::Name | OrderKey::Property(_) => row
                        .try_get::<Option<String>, _>(1)?
                        .map_or(Cursor::Missing, Cursor::Text),
                };
                Ok((row.try_get(0)?, key))
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(AppError::from)?;
        let next_page_token = if page_size > 0 && page.len() > page_size {
            page.truncate(page_size);
            let (id, key) = page.last().expect("a full page").clone();
            order.token(key, id)
        } else {
            String::new()
        };
        let ids: Vec<String> = page.into_iter().map(|(id, _)| id).collect();
        let entities = load_in_order(&self.pool, &ids, &mask).await?;
        Ok(ListEntitiesResponse {
            entities,
            next_page_token,
        })
    }

    /// CountReferences: for each id, the distinct entities that link to it,
    /// itself left out. Every id is in the result.
    pub(super) async fn reference_counts(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, u32>, Status> {
        let mut counts: HashMap<String, u32> = ids.iter().map(|id| (id.clone(), 0)).collect();
        if ids.is_empty() {
            return Ok(counts);
        }
        let ids_json = serde_json::to_string(ids).map_err(|e| AppError::Invalid(e.to_string()))?;
        let rows = sqlx::query!(
            r#"SELECT target_id AS "target_id!", COUNT(DISTINCT entity_id) AS "count!: i64"
               FROM links
               WHERE target_id IN (SELECT value FROM json_each(?1)) AND entity_id != target_id
               GROUP BY target_id"#,
            ids_json
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;
        for row in rows {
            counts.insert(row.target_id, u32::try_from(row.count).unwrap_or(u32::MAX));
        }
        Ok(counts)
    }

    /// MatchNames: the entities of `structure_types` (every structure when
    /// empty) whose folded name starts with the folded query, in folded-name
    /// order, then id.
    pub(super) async fn name_matches(&self, req: MatchNamesRequest) -> Result<Vec<Entity>, Status> {
        let MatchNamesRequest {
            query,
            structure_types,
            limit,
            read_mask,
        } = req;
        for structure_type in &structure_types {
            known_structure(structure_type)?;
        }
        let mask = ReadMask::parse(read_mask.as_ref())?;
        let structure_types = if structure_types.is_empty() {
            structures::STRUCTURES
                .iter()
                .map(|s| s.type_.to_string())
                .collect()
        } else {
            structure_types
        };
        let types_json = serde_json::to_string(&structure_types)
            .map_err(|e| AppError::Invalid(e.to_string()))?;
        // Every key that starts with `prefix` is in [prefix, prefix + U+10FFFF):
        // a range on `entities_name_key`.
        let prefix = fold_name(&query);
        let past = format!("{prefix}\u{10FFFF}");
        let limit = i64::from(if limit == 0 {
            DEFAULT_MATCH_LIMIT
        } else {
            limit
        });
        let ids = sqlx::query_scalar!(
            r#"SELECT id AS "id!" FROM entities
               WHERE structure_type IN (SELECT value FROM json_each(?1))
                 AND name_key >= ?2 AND name_key < ?3
               ORDER BY name_key, id LIMIT ?4"#,
            types_json,
            prefix,
            past,
            limit
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;
        Ok(load_in_order(&self.pool, &ids, &mask).await?)
    }

    /// ListCalendar: what falls on each day from `start` to `end`. The flagged
    /// properties come from the registry, so no structure is special here.
    pub(super) async fn calendar(
        &self,
        req: ListCalendarRequest,
    ) -> Result<ListCalendarResponse, Status> {
        let ListCalendarRequest {
            start,
            end,
            read_mask,
        } = req;
        for (field, day) in [("start", &start), ("end", &end)] {
            if !is_iso_day(day) {
                return Err(Status::invalid_argument(format!(
                    "{field} must be a yyyy-MM-dd day, not {day:?}"
                )));
            }
        }
        if end < start {
            return Err(Status::invalid_argument("end is before start"));
        }
        let mask = ReadMask::parse(read_mask.as_ref())?;
        let mut items: BTreeSet<(String, i32, String, String)> = BTreeSet::new();

        let flagged = serde_json::to_string(&structures::calendar_properties())
            .map_err(|e| AppError::Invalid(e.to_string()))?;
        let property_rows = sqlx::query!(
            r#"SELECT entity_id AS "entity_id!", property_id AS "property_id!",
                      value_key AS "day!"
               FROM properties
               WHERE property_id IN (SELECT value FROM json_each(?1))
                 AND value_key >= ?2 AND value_key <= ?3"#,
            flagged,
            start,
            end
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;
        for row in property_rows {
            let kind = CalendarItemKind::Property as i32;
            items.insert((row.day, kind, row.entity_id, row.property_id));
        }

        let reference_rows = sqlx::query!(
            r#"SELECT entity_id AS "entity_id!", iso_date AS "day!" FROM referenced_dates
               WHERE iso_date >= ? AND iso_date <= ?"#,
            start,
            end
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;
        for row in reference_rows {
            let kind = CalendarItemKind::Reference as i32;
            items.insert((row.day, kind, row.entity_id, String::new()));
        }

        // The local days from `start` to `end` are the instants from start's
        // local midnight to the one after end.
        let from = local_midnight(&start, 0)?;
        let until = local_midnight(&end, 1)?;
        let created_rows = sqlx::query!(
            r#"SELECT id AS "id!", created_at AS "created_at!" FROM entities
               WHERE created_at >= ? AND created_at < ?"#,
            from,
            until
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;
        for row in created_rows {
            let Some(created) = Local.timestamp_millis_opt(row.created_at).single() else {
                continue;
            };
            let day = created.date_naive().format("%Y-%m-%d").to_string();
            let kind = CalendarItemKind::Created as i32;
            items.insert((day, kind, row.id, String::new()));
        }

        let ids: Vec<String> = items
            .iter()
            .map(|(_, _, id, _)| id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let entities = load_in_order(&self.pool, &ids, &mask).await?;
        // An entity deleted between the queries takes its items with it.
        let present: HashSet<&str> = entities.iter().map(|e| e.id.as_str()).collect();
        let items = items
            .into_iter()
            .filter(|(_, _, id, _)| present.contains(id.as_str()))
            .map(|(day, kind, entity_id, property_id)| CalendarItem {
                day,
                kind,
                entity_id,
                property_id,
            })
            .collect();
        Ok(ListCalendarResponse { items, entities })
    }
}

/// Check a ListEntities filter and turn its values into value keys.
fn parse_filter(filter: PropertyFilter) -> Result<Filter, Status> {
    let PropertyFilter {
        property_id,
        condition,
    } = filter;
    let invalid = |why: String| Status::invalid_argument(format!("filter: {why}"));
    let kinds = scalar_property(&property_id).map_err(invalid)?;
    let key_of = |value: &calcifer::v1::PropertyValue| {
        value_key(value.value.as_ref()).ok_or_else(|| {
            invalid(format!(
                "{property_id}: compare with a date, select, text or number value"
            ))
        })
    };
    match condition {
        Some(Condition::Equals(value)) => {
            let key = key_of(&value)?;
            Ok(Filter::OneOf(property_id, vec![key]))
        }
        Some(Condition::OneOf(values)) => {
            let keys = values.values.iter().map(key_of).collect::<Result<_, _>>()?;
            Ok(Filter::OneOf(property_id, keys))
        }
        Some(Condition::DateRange(DateRange { start, end })) => {
            if !kinds.contains(&PropertyKind::Date) {
                return Err(invalid(format!("{property_id} isn't a date property")));
            }
            let bound = |day: String| -> Result<Option<String>, Status> {
                match day.as_str() {
                    "" => Ok(None),
                    d if is_iso_day(d) => Ok(Some(day)),
                    d => Err(invalid(format!("{d:?} isn't a yyyy-MM-dd day"))),
                }
            };
            Ok(Filter::Days(property_id, bound(start)?, bound(end)?))
        }
        None => Err(invalid(format!("{property_id}: a condition is required"))),
    }
}

/// Bind a cursor's key: a number for a timestamp, text otherwise.
fn push_cursor_key(sql: &mut QueryBuilder<Sqlite>, key: Cursor) {
    match key {
        Cursor::Millis(millis) => sql.push_bind(millis),
        Cursor::Text(text) => sql.push_bind(text),
        Cursor::Missing => sql.push("NULL"),
    };
}

/// The instant, in unix millis, of local midnight `days_after` days after the
/// ISO day `day`. A midnight a clock change skips is the first instant after it.
fn local_midnight(day: &str, days_after: u64) -> Result<i64, Status> {
    let date = NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.checked_add_days(chrono::Days::new(days_after)))
        .ok_or_else(|| Status::invalid_argument(format!("{day:?} is out of range")))?;
    let midnight = date.and_hms_opt(0, 0, 0).expect("midnight exists");
    let instant = Local
        .from_local_datetime(&midnight)
        .earliest()
        .or_else(|| {
            Local
                .from_local_datetime(&(midnight + chrono::Duration::hours(1)))
                .earliest()
        })
        .ok_or_else(|| Status::invalid_argument(format!("{day:?} has no local midnight")))?;
    Ok(instant.timestamp_millis())
}
