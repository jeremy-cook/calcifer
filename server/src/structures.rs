//! Minimal server-side structure metadata. The full registry (names, icons,
//! uniqueNames — exposed via RPC) lands with the MCP tools; for now this only
//! tells the get-or-create path which richtext/select properties a new entity
//! needs, and the link sync which properties are relations.
//! Tag name-uniqueness is enforced by the `one_tag_per_name` DB index, not here.

struct StructureMeta {
    type_: &'static str,
    richtext_properties: &'static [&'static str],
    select_defaults: &'static [(&'static str, &'static str)],
    relation_properties: &'static [&'static str],
}

const STRUCTURES: &[StructureMeta] = &[
    StructureMeta {
        type_: "Note",
        richtext_properties: &["content"],
        select_defaults: &[],
        relation_properties: &[],
    },
    StructureMeta {
        type_: "Tag",
        richtext_properties: &[],
        select_defaults: &[],
        relation_properties: &[],
    },
    StructureMeta {
        type_: "DailyNote",
        richtext_properties: &["content"],
        select_defaults: &[],
        relation_properties: &[],
    },
    StructureMeta {
        type_: "Todo",
        richtext_properties: &["content"],
        select_defaults: &[("status", "open"), ("priority", "none")],
        relation_properties: &["tags"],
    },
];

fn find(structure_type: &str) -> Option<&'static StructureMeta> {
    STRUCTURES.iter().find(|s| s.type_ == structure_type)
}

/// Richtext property ids a freshly-created entity of this structure should carry
/// (mirrors the FE's STRUCTURES). Unknown types get none.
pub fn richtext_properties(structure_type: &str) -> &'static [&'static str] {
    find(structure_type).map(|s| s.richtext_properties)
        .unwrap_or(&[])
}

/// Select property ids (and their default option keys) a freshly-created entity
/// of this structure should carry (mirrors the FE's STRUCTURES). Unknown types
/// and structures with no select properties get none.
pub fn select_defaults(structure_type: &str) -> &'static [(&'static str, &'static str)] {
    find(structure_type).map(|s| s.select_defaults)
        .unwrap_or(&[])
}

/// `relation`/`relations` property ids declared on this structure (mirrors the
/// FE's STRUCTURES). Link sync clears these even when absent from an update, so
/// removing the last ref drops its link rows. Unknown types get none.
pub fn relation_properties(structure_type: &str) -> &'static [&'static str] {
    find(structure_type).map(|s| s.relation_properties).unwrap_or(&[])
}
