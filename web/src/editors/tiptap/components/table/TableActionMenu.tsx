import { useEditorState, type Editor } from '@tiptap/react'
import { Button } from '~/components/ui/button'
import { Separator } from '~/components/ui/separator'

interface Props {
  editor: Editor
}

export function TableActionMenu({ editor }: Props) {
  const { isTable } = useEditorState({
    editor,
    selector: (ctx) => ({ isTable: ctx.editor.isActive('table') }),
  })

  if (!isTable) return null

  return (
    <div className="flex shrink-0 flex-wrap items-center gap-0.5 border-b border-border bg-muted/30 px-2 py-0.5">
      <span className="text-xs text-muted-foreground mr-1">Table:</span>
      <Button
        variant="ghost"
        size="sm"
        className="h-6 px-1.5 text-xs"
        onClick={() => editor.chain().focus().addRowBefore().run()}
      >
        + Row above
      </Button>
      <Button
        variant="ghost"
        size="sm"
        className="h-6 px-1.5 text-xs"
        onClick={() => editor.chain().focus().addRowAfter().run()}
      >
        + Row below
      </Button>
      <Button
        variant="ghost"
        size="sm"
        className="h-6 px-1.5 text-xs"
        onClick={() => editor.chain().focus().addColumnBefore().run()}
      >
        + Col left
      </Button>
      <Button
        variant="ghost"
        size="sm"
        className="h-6 px-1.5 text-xs"
        onClick={() => editor.chain().focus().addColumnAfter().run()}
      >
        + Col right
      </Button>
      <Separator orientation="vertical" className="mx-1 h-4" />
      <Button
        variant="ghost"
        size="sm"
        className="h-6 px-1.5 text-xs"
        onClick={() => editor.chain().focus().deleteRow().run()}
      >
        − Row
      </Button>
      <Button
        variant="ghost"
        size="sm"
        className="h-6 px-1.5 text-xs"
        onClick={() => editor.chain().focus().deleteColumn().run()}
      >
        − Col
      </Button>
      <Button
        variant="ghost"
        size="sm"
        className="h-6 px-1.5 text-xs"
        onClick={() => editor.chain().focus().mergeCells().run()}
      >
        Merge
      </Button>
      <Button
        variant="ghost"
        size="sm"
        className="h-6 px-1.5 text-xs"
        onClick={() => editor.chain().focus().splitCell().run()}
      >
        Split
      </Button>
      <Separator orientation="vertical" className="mx-1 h-4" />
      <Button
        variant="ghost"
        size="sm"
        className="h-6 px-1.5 text-xs text-destructive hover:text-destructive"
        onClick={() => editor.chain().focus().deleteTable().run()}
      >
        Delete table
      </Button>
    </div>
  )
}
