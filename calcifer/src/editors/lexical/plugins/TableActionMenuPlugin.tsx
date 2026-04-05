import { useEffect, useState } from 'react'
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext'
import { $getSelection, $isRangeSelection } from 'lexical'
import {
  $getTableCellNodeFromLexicalNode,
  $insertTableColumn__EXPERIMENTAL,
  $insertTableRow__EXPERIMENTAL,
  $deleteTableColumn__EXPERIMENTAL,
  $deleteTableRow__EXPERIMENTAL,
} from '@lexical/table'
import { Button } from '~/components/ui/button'
import { Separator } from '~/components/ui/separator'

export function TableActionMenuPlugin() {
  const [editor] = useLexicalComposerContext()
  const [isTableActive, setIsTableActive] = useState(false)

  useEffect(() => {
    return editor.registerUpdateListener(({ editorState }) => {
      editorState.read(() => {
        const selection = $getSelection()
        const next = $isRangeSelection(selection)
          ? $getTableCellNodeFromLexicalNode(selection.anchor.getNode()) !== null
          : false
        setIsTableActive((prev) => (prev === next ? prev : next))
      })
    })
  }, [editor])

  if (!isTableActive) return null

  return (
    <div className="flex shrink-0 flex-wrap items-center gap-0.5 border-b border-border bg-muted/30 px-2 py-0.5">
      <span className="text-xs text-muted-foreground mr-1">Table:</span>
      <Button variant="ghost" size="sm" className="h-6 px-1.5 text-xs" onClick={() => editor.update(() => $insertTableRow__EXPERIMENTAL(false))}>+ Row above</Button>
      <Button variant="ghost" size="sm" className="h-6 px-1.5 text-xs" onClick={() => editor.update(() => $insertTableRow__EXPERIMENTAL(true))}>+ Row below</Button>
      <Button variant="ghost" size="sm" className="h-6 px-1.5 text-xs" onClick={() => editor.update(() => $insertTableColumn__EXPERIMENTAL(false))}>+ Col left</Button>
      <Button variant="ghost" size="sm" className="h-6 px-1.5 text-xs" onClick={() => editor.update(() => $insertTableColumn__EXPERIMENTAL(true))}>+ Col right</Button>
      <Separator orientation="vertical" className="mx-1 h-4" />
      <Button variant="ghost" size="sm" className="h-6 px-1.5 text-xs" onClick={() => editor.update(() => $deleteTableRow__EXPERIMENTAL())}>− Row</Button>
      <Button variant="ghost" size="sm" className="h-6 px-1.5 text-xs" onClick={() => editor.update(() => $deleteTableColumn__EXPERIMENTAL())}>− Col</Button>
    </div>
  )
}
