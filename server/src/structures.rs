//! The structure registry: the one place structure data is authored (ADR 7).
//! `StructureService.List` serves it to the browser and the MCP server, and the
//! helpers below derive what the write paths need from the same table.
//! Tag name-uniqueness is enforced by the `one_tag_per_name` DB index, not here.

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
    pub unique_names: bool,
    pub name_editable: bool,
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
        unique_names: false,
        name_editable: true,
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
        unique_names: true,
        name_editable: true,
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
        // Created per day by CreateDailyNote, never from "+ New"; the name is the date.
        creatable: false,
        mentionable: false,
        unique_names: false,
        name_editable: false,
    },
    StructureDef {
        type_: "Todo",
        name: "To-do",
        plural: "To-dos",
        description:
            "an actionable item with status, priority, due date, and tags (reference with [[Name]])",
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
        unique_names: false,
        name_editable: true,
    },
];

pub fn structure(structure_type: &str) -> Option<&'static StructureDef> {
    STRUCTURES.iter().find(|s| s.type_ == structure_type)
}

/// A structure's declared property by id. Write validation of select values and
/// relation targets (I-2, I-9) looks definitions up here.
#[allow(dead_code)]
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

/// Richtext property ids a freshly-created entity of this structure should
/// carry. Unknown types get none.
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
            unique_names: def.unique_names,
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
