use tonic::Status;

use crate::dates::is_iso_day;
use crate::error::AppError;
use crate::proto::{property_value, Entity};
use crate::structures::{self, PropertyKind};

/// Check every property of an entity against its structure. See
/// `validate_property`.
pub(super) fn validate_properties(entity: &Entity) -> Result<(), AppError> {
    let facets: Vec<&str> = entity.facets.iter().map(String::as_str).collect();
    for (property_id, value) in &entity.properties {
        validate_property(
            &entity.structure_type,
            &facets,
            property_id,
            value.value.as_ref(),
        )?;
    }
    Ok(())
}

/// Reject a property value that doesn't fit the structure's declaration: a
/// declared rich-text property takes no value at all, since its document is
/// addressed by (entity id, property id) through `RichTextService` (I-22); any
/// other declared property must hold a value of its declared `PropertyKind`
/// (I-16), a declared select one of its options, and a `select` value is only
/// allowed on a declared select (I-9). A value on an undeclared property is
/// refused (I-79): a new property is a registry edit and a server restart.
/// Structures missing from the registry (rows written before unknown types were
/// rejected, I-24) declare nothing, so they take no values. A `date` value must
/// be an ISO day (I-52). Shared by Create and Resolve (via
/// `validate_properties`) and SetProperty.
///
/// `facets` are the entity's attached facets (ADR 11): a facet property is
/// declared only while its facet is attached, and a value on an id in a
/// registered facet's namespace (`todo.status`) whose facet isn't attached is
/// `FailedPrecondition` (D6). Callers don't validate a clear, so clearing one
/// stays allowed.
pub(super) fn validate_property(
    structure_type: &str,
    facets: &[&str],
    property_id: &str,
    value: Option<&property_value::Value>,
) -> Result<(), AppError> {
    if let Some(facet) = structures::facet_of_property(property_id) {
        if value.is_some() && !facets.contains(&facet.id) {
            return Err(AppError::FailedPrecondition(format!(
                "attach the {} facet before setting {property_id}",
                facet.id
            )));
        }
    }
    let Some(def) = structures::property_for(structure_type, facets, property_id) else {
        return Err(AppError::Invalid(format!(
            "{structure_type}.{property_id} isn't a declared property"
        )));
    };
    if let Some(property_value::Value::Date(d)) = value {
        if !is_iso_day(d) {
            return Err(AppError::Invalid(format!(
                "{structure_type}.{property_id} must be an ISO calendar day (yyyy-MM-dd), got {d:?}"
            )));
        }
    }
    // Exhaustive, so a new kind must decide how it's checked.
    let expected = match def.kind {
        PropertyKind::Richtext => {
            return Err(AppError::Invalid(format!(
                "{structure_type}.{property_id} is a rich-text property and takes no value; \
                 its document is read and written with RichTextService"
            )));
        }
        PropertyKind::Select => return validate_select(structure_type, def, value),
        PropertyKind::Text => "text",
        PropertyKind::Number => "number",
        PropertyKind::Date => "date",
        PropertyKind::Relation => "relation",
        PropertyKind::Relations => "relations",
    };
    let got = value.map(case_name);
    if got != Some(expected) {
        return Err(AppError::Invalid(format!(
            "{structure_type}.{property_id} must be a {expected} value, got {}",
            got.unwrap_or("no value")
        )));
    }
    Ok(())
}

/// A declared select's value must be a `select` naming one of its options (I-9).
pub(super) fn validate_select(
    structure_type: &str,
    def: &structures::PropertyDef,
    value: Option<&property_value::Value>,
) -> Result<(), AppError> {
    let allowed: Vec<&str> = def.options.iter().map(|o| o.key).collect();
    let Some(property_value::Value::Select(key)) = value else {
        return Err(AppError::Invalid(format!(
            "{structure_type}.{} must be a select value, one of: {}",
            def.id,
            allowed.join(", ")
        )));
    };
    if !allowed.contains(&key.as_str()) {
        return Err(AppError::Invalid(format!(
            "invalid value {key:?} for {structure_type}.{}; allowed: {}",
            def.id,
            allowed.join(", ")
        )));
    }
    Ok(())
}

/// A value's case as it is spelled in the proto, which is also the name of the
/// `PropertyKind` that takes it.
pub(super) fn case_name(value: &property_value::Value) -> &'static str {
    match value {
        property_value::Value::Text(_) => "text",
        property_value::Value::Number(_) => "number",
        property_value::Value::Date(_) => "date",
        property_value::Value::Select(_) => "select",
        property_value::Value::Relation(_) => "relation",
        property_value::Value::Relations(_) => "relations",
    }
}

/// `InvalidArgument` for a property id that starts with `@`. Those ids are
/// reserved for the link scopes the server derives, such as `@related` (ADR 12,
/// rule 9), so no write may name one as a property.
pub(crate) fn reject_reserved_property_id(id: &str) -> Result<(), Status> {
    if id.starts_with('@') {
        return Err(Status::invalid_argument(
            "property ids starting with @ are reserved",
        ));
    }
    Ok(())
}

/// `InvalidArgument` if `name` (already trimmed) contains `|` or `]]`. The agent's
/// link syntax (`[[Name|Alias]]`, closed by `]]`) can't express such a name, so no
/// path that sets one accepts it (I-61). A lone `[` or `]` is fine: only `]]`
/// closes a link. The MCP `create_note` calls it too, before it writes anything.
pub(crate) fn check_name_chars(name: &str) -> Result<(), Status> {
    if name.contains('|') || name.contains("]]") {
        return Err(Status::invalid_argument(
            r#"names can't contain "|" or "]]""#,
        ));
    }
    Ok(())
}
