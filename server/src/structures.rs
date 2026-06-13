//! Minimal server-side structure metadata. The full registry (names, icons,
//! uniqueNames — exposed via RPC) lands with the MCP tools; for now this only
//! tells the get-or-create path which richtext properties a new entity needs.
//! Tag name-uniqueness is enforced by the `one_tag_per_name` DB index, not here.

struct StructureMeta {
    type_: &'static str,
    richtext_properties: &'static [&'static str],
}

const STRUCTURES: &[StructureMeta] = &[
    StructureMeta { type_: "Note", richtext_properties: &["content"] },
    StructureMeta { type_: "Tag", richtext_properties: &[] },
    StructureMeta { type_: "DailyNote", richtext_properties: &["content"] },
];

/// Richtext property ids a freshly-created entity of this structure should carry
/// (mirrors the FE's STRUCTURES). Unknown types get none.
pub fn richtext_properties(structure_type: &str) -> &'static [&'static str] {
    STRUCTURES
        .iter()
        .find(|s| s.type_ == structure_type)
        .map(|s| s.richtext_properties)
        .unwrap_or(&[])
}
