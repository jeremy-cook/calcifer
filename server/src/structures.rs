//! The structure registry: the one place structure data is authored (ADR 7).
//! `StructureService.ListStructures` serves it to the browser, the MCP tools read
//! it directly, and the helpers below derive what the write paths need from it.
//! A structure's natural keys (`keys`) are enforced by the `entity_keys` table
//! (ADR 10): the registry declares them, the write paths fill the table.

use crate::proto;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind {
    Richtext,
    Text,
    Number,
    Date,
    Select,
    Relation,
    Relations,
}

#[derive(Debug)]
pub struct SelectOption {
    pub key: &'static str,
    pub label: &'static str,
}

#[derive(Debug)]
pub struct PropertyDef {
    pub id: &'static str,
    pub kind: PropertyKind,
    /// Empty = none; clients fall back to the id.
    pub label: &'static str,
    /// Select only, in display (and priority-rank) order.
    pub options: &'static [SelectOption],
    /// Select only.
    pub default_option: Option<&'static str>,
    /// Relation/relations only; `None` = any structure.
    pub target_structure: Option<&'static str>,
}

/// Where a natural key's value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// The entity's name, trimmed and lowercased (see `fold_name`).
    Name,
    /// A property's value, already canonical (a date is an ISO day).
    Property(&'static str),
}

/// A natural key: a value that identifies an entity of the structure besides its
/// id. No two entities of a structure share a key's value.
#[derive(Debug)]
pub struct KeyDef {
    pub key: &'static str,
    pub source: KeySource,
}

#[derive(Debug)]
pub struct StructureDef {
    pub type_: &'static str,
    pub name: &'static str,
    pub plural: &'static str,
    /// One line, for the agent (`list_structures`).
    pub description: &'static str,
    pub properties: &'static [PropertyDef],
    pub creatable: bool,
    pub mentionable: bool,
    pub name_editable: bool,
    pub keys: &'static [KeyDef],
}

impl StructureDef {
    /// Whether the (folded) name is the identity: the structure declares a key
    /// sourced from its name.
    pub fn unique_names(&self) -> bool {
        self.name_key().is_some()
    }

    /// The key sourced from the name, if the structure declares one.
    pub fn name_key(&self) -> Option<&'static KeyDef> {
        self.keys.iter().find(|k| k.source == KeySource::Name)
    }
}

/// The canonical form of a name used as a key: trimmed and lowercased, with
/// Unicode case folding (unlike SQLite's `NOCASE`, which only folds ASCII).
pub fn fold_name(name: &str) -> String {
    name.trim().to_lowercase()
}

const fn prop(id: &'static str, kind: PropertyKind, label: &'static str) -> PropertyDef {
    PropertyDef {
        id,
        kind,
        label,
        options: &[],
        default_option: None,
        target_structure: None,
    }
}

const fn select(
    id: &'static str,
    label: &'static str,
    options: &'static [SelectOption],
    default_option: &'static str,
) -> PropertyDef {
    PropertyDef {
        options,
        default_option: Some(default_option),
        ..prop(id, PropertyKind::Select, label)
    }
}

const fn relations(id: &'static str, label: &'static str, target: &'static str) -> PropertyDef {
    PropertyDef {
        target_structure: Some(target),
        ..prop(id, PropertyKind::Relations, label)
    }
}

const fn option(key: &'static str, label: &'static str) -> SelectOption {
    SelectOption { key, label }
}

/// Every structure, in the order clients list them (sidebar, backlink groups).
pub const STRUCTURES: &[StructureDef] = &[
    StructureDef {
        type_: "Note",
        name: "Note",
        plural: "Notes",
        description: "a concept/topic note (the main building block; reference with [[Name]])",
        properties: &[prop("content", PropertyKind::Richtext, "")],
        creatable: true,
        mentionable: true,
        name_editable: true,
        keys: &[],
    },
    StructureDef {
        type_: "Tag",
        name: "Tag",
        plural: "Tags",
        description: "a label, written #tag in note content",
        properties: &[],
        creatable: true,
        // Reached via `#`, not bare `@`.
        mentionable: false,
        name_editable: true,
        keys: &[KeyDef {
            key: "name",
            source: KeySource::Name,
        }],
    },
    StructureDef {
        type_: "DailyNote",
        name: "Daily Note",
        plural: "Daily Notes",
        description: "a journal entry for a calendar day",
        properties: &[
            prop("date", PropertyKind::Date, ""),
            prop("content", PropertyKind::Richtext, ""),
        ],
        // Created per day by Resolve's date key, never from "+ New"; the server
        // derives the name from the date.
        creatable: false,
        mentionable: false,
        name_editable: false,
        keys: &[KeyDef {
            key: "date",
            source: KeySource::Property("date"),
        }],
    },
    StructureDef {
        type_: "Todo",
        name: "To-do",
        plural: "To-dos",
        description:
            "an actionable item with status, priority, due date, and tags (reference with [[Todo/Name]])",
        properties: &[
            select(
                "status",
                "Status",
                &[option("open", "Open"), option("done", "Done")],
                "open",
            ),
            select(
                "priority",
                "Priority",
                &[
                    option("none", "None"),
                    option("low", "Low"),
                    option("medium", "Medium"),
                    option("high", "High"),
                ],
                "none",
            ),
            prop("due", PropertyKind::Date, "Due"),
            relations("tags", "Tags", "Tag"),
            prop("content", PropertyKind::Richtext, ""),
        ],
        creatable: true,
        mentionable: true,
        name_editable: true,
        keys: &[],
    },
];

pub fn structure(structure_type: &str) -> Option<&'static StructureDef> {
    STRUCTURES.iter().find(|s| s.type_ == structure_type)
}

/// A structure's declared property by id. Write validation of select values and
/// relation targets (I-2, I-9) looks definitions up here.
pub fn property(structure_type: &str, id: &str) -> Option<&'static PropertyDef> {
    structure(structure_type)?
        .properties
        .iter()
        .find(|p| p.id == id)
}

fn properties_of(structure_type: &str) -> impl Iterator<Item = &'static PropertyDef> {
    structure(structure_type)
        .map(|s| s.properties)
        .unwrap_or(&[])
        .iter()
}

/// A structure's declared rich-text property ids: the property half of every
/// `RichTextRef` it can have (I-22). Unknown types get none.
pub fn richtext_properties(structure_type: &str) -> Vec<&'static str> {
    properties_of(structure_type)
        .filter(|p| p.kind == PropertyKind::Richtext)
        .map(|p| p.id)
        .collect()
}

/// Select property ids (and their default option keys) a freshly-created entity
/// of this structure should carry. Unknown types and structures with no
/// defaulted select properties get none.
pub fn select_defaults(structure_type: &str) -> Vec<(&'static str, &'static str)> {
    properties_of(structure_type)
        .filter(|p| p.kind == PropertyKind::Select)
        .filter_map(|p| p.default_option.map(|d| (p.id, d)))
        .collect()
}

/// `relation`/`relations` property ids declared on this structure. Link sync
/// clears these even when absent from an update, so removing the last ref drops
/// its link rows. Unknown types get none.
pub fn relation_properties(structure_type: &str) -> Vec<&'static str> {
    properties_of(structure_type)
        .filter(|p| matches!(p.kind, PropertyKind::Relation | PropertyKind::Relations))
        .map(|p| p.id)
        .collect()
}

impl From<PropertyKind> for proto::PropertyKind {
    fn from(kind: PropertyKind) -> Self {
        match kind {
            PropertyKind::Richtext => proto::PropertyKind::Richtext,
            PropertyKind::Text => proto::PropertyKind::Text,
            PropertyKind::Number => proto::PropertyKind::Number,
            PropertyKind::Date => proto::PropertyKind::Date,
            PropertyKind::Select => proto::PropertyKind::Select,
            PropertyKind::Relation => proto::PropertyKind::Relation,
            PropertyKind::Relations => proto::PropertyKind::Relations,
        }
    }
}

impl From<&PropertyDef> for proto::PropertyDef {
    fn from(def: &PropertyDef) -> Self {
        proto::PropertyDef {
            id: def.id.to_string(),
            kind: proto::PropertyKind::from(def.kind).into(),
            label: def.label.to_string(),
            options: def
                .options
                .iter()
                .map(|o| proto::SelectOption {
                    key: o.key.to_string(),
                    label: o.label.to_string(),
                })
                .collect(),
            default_option: def.default_option.unwrap_or_default().to_string(),
            target_structure: def.target_structure.unwrap_or_default().to_string(),
        }
    }
}

impl From<&StructureDef> for proto::StructureDef {
    fn from(def: &StructureDef) -> Self {
        proto::StructureDef {
            r#type: def.type_.to_string(),
            name: def.name.to_string(),
            plural: def.plural.to_string(),
            description: def.description.to_string(),
            properties: def.properties.iter().map(Into::into).collect(),
            creatable: def.creatable,
            mentionable: def.mentionable,
            unique_names: def.unique_names(),
            name_editable: def.name_editable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The derived helpers must keep returning what the hand-written per-structure
    // lists did before the registry replaced them.
    #[test]
    fn derived_helpers_match_previous_lists() {
        assert_eq!(richtext_properties("Note"), ["content"]);
        assert!(richtext_properties("Tag").is_empty());
        assert_eq!(richtext_properties("DailyNote"), ["content"]);
        assert_eq!(richtext_properties("Todo"), ["content"]);
        assert!(richtext_properties("Nope").is_empty());

        assert!(select_defaults("Note").is_empty());
        assert!(select_defaults("Tag").is_empty());
        assert!(select_defaults("DailyNote").is_empty());
        assert_eq!(
            select_defaults("Todo"),
            [("status", "open"), ("priority", "none")]
        );
        assert!(select_defaults("Nope").is_empty());

        assert!(relation_properties("Note").is_empty());
        assert!(relation_properties("Tag").is_empty());
        assert!(relation_properties("DailyNote").is_empty());
        assert_eq!(relation_properties("Todo"), ["tags"]);
        assert!(relation_properties("Nope").is_empty());
    }

    #[test]
    fn property_def_looks_up_by_structure_and_id() {
        let priority = property("Todo", "priority").expect("Todo.priority");
        assert_eq!(priority.kind, PropertyKind::Select);
        assert_eq!(priority.default_option, Some("none"));
        let tags = property("Todo", "tags").expect("Todo.tags");
        assert_eq!(tags.target_structure, Some("Tag"));
        assert!(property("Todo", "nope").is_none());
        assert!(property("Nope", "content").is_none());
    }

    #[test]
    fn property_keys_name_declared_properties() {
        for s in STRUCTURES {
            for k in s.keys {
                if let KeySource::Property(id) = k.source {
                    assert!(
                        property(s.type_, id).is_some(),
                        "{} key {:?} names undeclared property {id:?}",
                        s.type_,
                        k.key
                    );
                }
            }
        }
    }

    #[test]
    fn unique_names_follows_a_name_key() {
        let unique: Vec<&str> = STRUCTURES
            .iter()
            .filter(|s| s.unique_names())
            .map(|s| s.type_)
            .collect();
        assert_eq!(unique, ["Tag"]);
    }

    #[test]
    fn fold_name_trims_and_folds_unicode_case() {
        assert_eq!(fold_name("  Reading "), "reading");
        assert_eq!(fold_name("CAFÉ"), fold_name("Café"));
    }

    #[test]
    fn select_defaults_are_declared_options() {
        for s in STRUCTURES {
            for p in s.properties {
                if let Some(default) = p.default_option {
                    assert!(
                        p.options.iter().any(|o| o.key == default),
                        "{}.{} default {default:?} is not one of its options",
                        s.type_,
                        p.id
                    );
                }
            }
        }
    }
}
