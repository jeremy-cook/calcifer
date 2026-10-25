use std::collections::HashMap;

use super::keys::property_key_value;
use super::lookup::sort_facets;
use crate::dates::format_long_date;
use crate::proto::{property_value, Entity, PropertyValue};
use crate::structures::{self, FacetDef};

/// The DailyNote name rule (ADR 8): a DailyNote is named for its date, as a long
/// human date, on every write. `date` is the value of a DailyNote's `date`
/// property, so this is None for an undated DailyNote, which keeps the name it
/// has.
pub(super) fn daily_note_name(date: Option<&str>) -> Option<String> {
    date.map(format_long_date)
}

/// Apply `daily_note_name` to a whole entity, for `new_entity` (and so Create and
/// Resolve). SetProperty applies it to the one row.
pub(super) fn apply_daily_note_name(entity: &mut Entity) {
    if entity.structure_type != "DailyNote" {
        return;
    }
    let date = property_key_value(entity.properties.get("date").and_then(|v| v.value.as_ref()));
    if let Some(name) = daily_note_name(date.as_deref()) {
        entity.name = name;
    }
}

/// Build a new, unsaved entity with every server-owned default (ADR 8): a minted
/// id, the `facets` (ADR 11, in registry order), and each select's default
/// option, the structure's and the facets', with the caller's `properties` laid
/// over them (the caller wins on the same id). Rich-text properties get no value: their
/// documents are addressed by declared property (I-22), and `validate_property`
/// rejects a caller value for one on persist. A DailyNote with a date is named for it
/// (`apply_daily_note_name`); otherwise the name is `name`, or with none the
/// structure's `default_name`. The caller makes a `unique_names` default free
/// (`EntityService::free_name`); this has no database.
pub(super) fn new_entity(
    structure_type: &str,
    name: Option<&str>,
    properties: HashMap<String, PropertyValue>,
    facets: &[&FacetDef],
) -> Entity {
    let id = uuid::Uuid::new_v4().to_string();
    let mut select_defaults = structures::select_defaults(structure_type);
    for facet in facets {
        select_defaults.extend(structures::facet_select_defaults(facet));
    }
    let mut facets: Vec<String> = facets.iter().map(|f| f.id.to_string()).collect();
    sort_facets(&mut facets);
    let mut defaults: HashMap<String, PropertyValue> = select_defaults
        .iter()
        .map(|(pid, default)| {
            let value = PropertyValue {
                value: Some(property_value::Value::Select(default.to_string())),
            };
            (pid.to_string(), value)
        })
        .collect();
    defaults.extend(properties);
    let mut entity = Entity {
        id,
        structure_type: structure_type.to_string(),
        name: name.map_or_else(|| default_name(structure_type), str::to_string),
        properties: defaults,
        links: vec![],
        referenced_dates: vec![],
        created_at: None,
        updated_at: None,
        facets,
    };
    apply_daily_note_name(&mut entity);
    entity
}

/// The name of a new entity created without one: `Untitled <StructureDef.name>`,
/// e.g. "Untitled Note". Structures missing from the registry use their type.
pub(super) fn default_name(structure_type: &str) -> String {
    let name = structures::structure(structure_type).map_or(structure_type, |s| s.name);
    format!("Untitled {name}")
}

/// A DailyNote's properties: just its `date`, holding the ISO day `date`.
pub(super) fn date_properties(date: &str) -> HashMap<String, PropertyValue> {
    let value = PropertyValue {
        value: Some(property_value::Value::Date(date.to_string())),
    };
    HashMap::from([("date".to_string(), value)])
}
