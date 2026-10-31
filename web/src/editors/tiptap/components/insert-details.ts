import { findParentNode, type Editor, type Range } from '@tiptap/core'
import { TextSelection } from '@tiptap/pm/state'

// Inserts a Details block at the selection: wrapped around the selected blocks,
// or, inside a table, after the table, since wrapping a cell's content would
// hide it. With `range` (a slash command's text), that is deleted first. The
// caret ends up in the block's summary.
export function insertDetails(editor: Editor, range?: Range): boolean {
  const chain = editor.chain().focus()
  if (range) chain.deleteRange(range)
  return chain
    .command(({ tr, state, commands }) => {
      const table = findParentNode((node) => node.type.name === 'table')(tr.selection)
      if (!table) return commands.setDetails()
      const details = state.schema.nodes.details.createAndFill()
      if (!details) return false
      const after = table.pos + table.node.nodeSize
      tr.insert(after, details)
      // Inside the summary, as `setDetails` leaves it.
      tr.setSelection(TextSelection.create(tr.doc, after + 2))
      return true
    })
    .run()
}
