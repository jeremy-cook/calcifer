use serde_json::Value;

use crate::mcp::markdown::{self};

use super::{fail, ToolError};

/// Where one top-level block lands in the rendered body: its index in the doc's
/// content and its 1-based first and last body line. An empty block renders
/// nothing, so it has no span.
#[derive(Debug, Clone, Copy)]
pub(super) struct BlockSpan {
    pub(super) index: usize,
    pub(super) first: usize,
    pub(super) last: usize,
}

/// The doc blocks `from..to` that body lines `start..=end` cover; an empty range
/// (end = start - 1) is an insertion point. A line range may start and end on
/// blank lines but never inside a block.
pub(super) fn block_range(
    spans: &[BlockSpan],
    block_count: usize,
    start: usize,
    end: usize,
) -> Result<(usize, usize), ToolError> {
    let total = spans.last().map_or(0, |s| s.last);
    if start > total + 1 {
        return fail(format!(
            "start {start} is outside the note: its body has {total} line(s), so start must be 1 to {}.",
            total + 1
        ));
    }
    if end + 1 < start || end > total {
        return fail(format!(
            "end {end} is outside the note: it must be {} (insert before line {start}) to {total}.",
            start - 1
        ));
    }
    let first = spans.iter().find(|s| s.last >= start);
    if let Some(f) = first.filter(|f| start > f.first) {
        return fail(format!(
            "line {start} is inside the block at lines {}-{}; edits take whole blocks, so start at {}.",
            f.first, f.last, f.first
        ));
    }
    if end + 1 == start {
        let at = first.map_or(block_count, |f| f.index);
        return Ok((at, at));
    }
    let last = spans.iter().rfind(|s| s.first <= end);
    let (Some(first), Some(last)) = (first, last) else {
        return fail(format!(
            "lines {start}-{end} contain no block; to insert there, use end = {}.",
            start - 1
        ));
    };
    if last.index < first.index {
        return fail(format!(
            "lines {start}-{end} contain no block; to insert there, use end = {}.",
            start - 1
        ));
    }
    if end < last.last {
        return fail(format!(
            "line {end} is inside the block at lines {}-{}; edits take whole blocks, so end at {}.",
            last.first, last.last, last.last
        ));
    }
    Ok((first.index, last.index + 1))
}

pub(super) fn is_empty_paragraph(block: &Value) -> bool {
    block["type"] == "paragraph" && block["content"].as_array().is_none_or(Vec::is_empty)
}

/// Blocks `from..to` are about to be replaced from markdown; refuse if one holds
/// a node or formatting markdown can't express, which the replacement would
/// silently drop.
pub(super) fn refuse_inexpressible(
    spans: &[BlockSpan],
    blocks: &[Value],
    from: usize,
    to: usize,
) -> Result<(), ToolError> {
    for span in spans.iter().filter(|s| (from..to).contains(&s.index)) {
        if let Some(what) = markdown::inexpressible(&blocks[span.index]) {
            return fail(format!(
                "lines {}-{} hold {what} that markdown can't express; leave them out of the range.",
                span.first, span.last
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans() -> Vec<BlockSpan> {
        // Blocks on lines 1, 3, 5-6 and 8.
        [(0, 1, 1), (1, 3, 3), (2, 5, 6), (3, 8, 8)]
            .map(|(index, first, last)| BlockSpan { index, first, last })
            .to_vec()
    }

    fn range(start: usize, end: usize) -> Result<(usize, usize), String> {
        block_range(&spans(), 4, start, end).map_err(|e| e.0)
    }

    #[test]
    fn block_range_maps_lines_onto_blocks() {
        assert_eq!(range(3, 3), Ok((1, 2)));
        assert_eq!(range(5, 6), Ok((2, 3)));
        assert_eq!(range(2, 4), Ok((1, 2)), "blank edges are fine");
        assert_eq!(range(1, 8), Ok((0, 4)));
        assert_eq!(range(1, 0), Ok((0, 0)), "insert before line 1");
        assert_eq!(range(9, 8), Ok((4, 4)), "start = last + 1 appends");
        assert_eq!(
            range(4, 3),
            Ok((2, 2)),
            "inserting on a blank line goes before the next block"
        );
    }

    #[test]
    fn block_range_rejects_what_cuts_a_block() {
        assert!(range(6, 6).unwrap_err().contains(
            "line 6 is inside the block at lines 5-6; edits take whole blocks, so start at 5"
        ));
        assert!(range(5, 5).unwrap_err().contains(
            "line 5 is inside the block at lines 5-6; edits take whole blocks, so end at 6"
        ));
        assert!(range(2, 2).unwrap_err().contains("contain no block"));
        assert!(range(99, 99).unwrap_err().contains("start 99 is outside"));
        assert!(range(1, 99).unwrap_err().contains("end 99 is outside"));
        assert!(range(3, 1).unwrap_err().contains("end 1 is outside"));
    }
}
