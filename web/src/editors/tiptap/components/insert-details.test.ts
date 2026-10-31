// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest'
import { Editor, type JSONContent } from '@tiptap/core'
import { StarterKit } from '@tiptap/starter-kit'
import { Details, DetailsContent, DetailsSummary } from '@tiptap/extension-details'
import { Table } from '@tiptap/extension-table'
import { TableRow } from '@tiptap/extension-table-row'
import { TableCell } from '@tiptap/extension-table-cell'
import { TableHeader } from '@tiptap/extension-table-header'
import { insertDetails } from '~/editors/tiptap/components/insert-details'
import { SLASH_ITEMS } from '~/editors/tiptap/components/slash-menu/slash-items'

const paragraph = (text: string): JSONContent => ({ type: 'paragraph', content: [{ type: 'text', text }] })

const tableWithCell = (text: string): JSONContent => ({
  type: 'table',
  content: [{ type: 'tableRow', content: [{ type: 'tableCell', content: [paragraph(text)] }, { type: 'tableCell' }] }],
})

let editor: Editor

// An editor with `content`, and the caret inside the first occurrence of `text`.
function editorWith(content: JSONContent[], caretIn: string) {
  editor = new Editor({
    element: document.createElement('div'),
    extensions: [StarterKit, Details, DetailsSummary, DetailsContent, Table, TableRow, TableHeader, TableCell],
    content: { type: 'doc', content },
  })
  let pos = 0
  editor.state.doc.descendants((node, at) => {
    if (node.isText && node.text === caretIn) pos = at + 1
  })
  editor.commands.setTextSelection(pos)
}

const topLevel = () => (editor.getJSON().content ?? []).map((node) => node.type)

afterEach(() => editor.destroy())

describe('insertDetails', () => {
  it('leaves a table cell as it was and adds the block after the table (I-88)', () => {
    editorWith([paragraph('before'), tableWithCell('cell text'), paragraph('after')], 'cell text')

    insertDetails(editor)

    expect(topLevel()).toEqual(['paragraph', 'table', 'details', 'paragraph'])
    const json = editor.getJSON().content!
    expect(JSON.stringify(json[1])).toContain('cell text')
    expect(JSON.stringify(json[2])).not.toContain('cell text')
    expect(editor.state.selection.$from.parent.type.name).toBe('detailsSummary')
  })

  it('adds the block after a table that ends the doc', () => {
    editorWith([tableWithCell('cell text')], 'cell text')

    insertDetails(editor)

    expect(topLevel().slice(0, 2)).toEqual(['table', 'details'])
  })

  it('still wraps the block when the caret is outside a table', () => {
    editorWith([paragraph('wrap me'), tableWithCell('cell text')], 'wrap me')

    insertDetails(editor)

    expect(topLevel().slice(0, 2)).toEqual(['details', 'table'])
    expect(JSON.stringify(editor.getJSON().content![0])).toContain('wrap me')
  })

  it('deletes the slash command text and inserts after the table', () => {
    editorWith([tableWithCell('cell /details')], 'cell /details')
    const to = editor.state.selection.$from.end()
    const range = { from: to - '/details'.length, to }

    SLASH_ITEMS.find((item) => item.title === 'Details')!.command({ editor, range })

    expect(topLevel().slice(0, 2)).toEqual(['table', 'details'])
    expect(JSON.stringify(editor.getJSON().content![0])).toContain('"cell "')
    expect(JSON.stringify(editor.getJSON().content![0])).not.toContain('/details')
  })
})
