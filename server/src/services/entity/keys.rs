use std::collections::HashMap;

use tonic::Status;

use crate::error::AppError;
use crate::structures::{self, fold_name, KeyDef, KeySource};
use calcifer::v1::{property_value, PropertyValue};

/// The natural keys a structure declares (ADR 10). Structures missing from the
/// registry have none.
pub(super) fn declared_keys(structure_type: &str) -> &'static [KeyDef] {
    structures::structure(structure_type).map_or(&[], |s| s.keys)
}

/// The canonical value of one natural key, from the entity's `name` and
/// `properties`. None when the source is empty, e.g. a DailyNote with no date.
pub(super) fn key_value(
    source: KeySource,
    name: &str,
    properties: &HashMap<String, PropertyValue>,
) -> Option<String> {
    match source {
        KeySource::Name => Some(fold_name(name)).filter(|n| !n.is_empty()),
        KeySource::Property(id) => {
            property_key_value(properties.get(id).and_then(|v| v.value.as_ref()))
        }
    }
}

/// The key value a single property contributes, for SetProperty, which only sees
/// the one property. A key's property is canonical once `validate_property` has
/// passed it (a date is an ISO day), so it's used as is.
pub(super) fn property_key_value(value: Option<&property_value::Value>) -> Option<String> {
    match value {
        Some(
            property_value::Value::Date(v)
            | property_value::Value::Text(v)
            | property_value::Value::Select(v),
        ) if !v.is_empty() => Some(v.clone()),
        _ => None,
    }
}

/// How a key value is shown in a conflict message: a name as the user typed it,
/// not folded; any other value as stored.
pub(super) fn shown_key_value<'a>(
    source: KeySource,
    name: &'a str,
    value: Option<&'a str>,
) -> &'a str {
    match source {
        KeySource::Name => name,
        KeySource::Property(_) => value.unwrap_or_default(),
    }
}

/// Replace an entity's value for one key: delete its `(entity_id, key)` row, then
/// insert `value` if there is one. A value another entity of the structure holds
/// fails the `entity_keys` primary key (see `map_unique_violation`). Call inside
/// the same tx as the write that changed the key's source.
pub(super) async fn set_key(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entity_id: &str,
    structure_type: &str,
    key: &str,
    value: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM entity_keys WHERE entity_id = ? AND key = ?",
        entity_id,
        key
    )
    .execute(&mut **tx)
    .await?;
    if let Some(value) = value {
        sqlx::query!(
            "INSERT INTO entity_keys (structure_type, key, value, entity_id) VALUES (?, ?, ?, ?)",
            structure_type,
            key,
            value,
            entity_id,
        )
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Map a `set_key` failure: a value another entity of the structure already
/// holds (the `entity_keys` primary key) is `AlreadyExists` with a message for
/// the key (I-25); anything else, including a clash on `(entity_id, key)`, which
/// would be a bug in `set_key`'s delete-then-insert, takes the standard AppError
/// -> Status conversion. `shown` is the value as the user gave it.
pub(super) fn map_unique_violation(
    err: AppError,
    structure_type: &str,
    key: &KeyDef,
    shown: &str,
) -> Status {
    let AppError::Db(sqlx::Error::Database(ref db)) = err else {
        return Status::from(err);
    };
    // SQLite names a failed unique constraint by its column list. The tests
    // below pin the messages.
    if !db.is_unique_violation()
        || !db
            .message()
            .ends_with("entity_keys.structure_type, entity_keys.key, entity_keys.value")
    {
        return Status::from(err);
    }
    let message = match (structure_type, key.key, key.source) {
        ("DailyNote", "date", _) => format!("a DailyNote for {shown} already exists"),
        (_, _, KeySource::Name) => format!("a {structure_type} named \"{shown}\" already exists"),
        (_, key, _) => format!("a {structure_type} with {key} {shown:?} already exists"),
    };
    Status::already_exists(message)
}
