import { Fragment } from 'react'
import { useEditorState, type Editor } from '@tiptap/react'
import {
  ArrowsInLineHorizontalIcon,
  ArrowsOutLineHorizontalIcon,
  ColumnsPlusLeftIcon,
  ColumnsPlusRightIcon,
  MinusSquareIcon,
  RowsPlusBottomIcon,
  RowsPlusTopIcon,
  TableIcon,
  TrashIcon,
  type Icon,
} from '@phosphor-icons/react'
import { toggleVariants } from '~/components/ui/toggle'
import { cn } from '~/lib/utils'

interface TableActionMenuProps {
  editor: Editor
}

interface TableAction {
  label: string
  icon: Icon
  run: () => void
  danger?: boolean
}

// A second toolbar row while the cursor is in a table: rows, columns and cells as icon
// buttons in the toolbar's style, names on hover, Delete table last in red.
export function TableActionMenu({ editor }: TableActionMenuProps) {
  const { isTable } = useEditorState({
    editor,
    selector: (ctx) => ({ isTable: ctx.editor.isActive('table') }),
  })

  if (!isTable) return null

  const chain = () => editor.chain().focus()
  const groups: TableAction[][] = [
    [
      { label: 'Add row above', icon: RowsPlusTopIcon, run: () => chain().addRowBefore().run() },
      { label: 'Add row below', icon: RowsPlusBottomIcon, run: () => chain().addRowAfter().run() },
      { label: 'Delete row', icon: MinusSquareIcon, run: () => chain().deleteRow().run() },
    ],
    [
      { label: 'Add column left', icon: ColumnsPlusLeftIcon, run: () => chain().addColumnBefore().run() },
      { label: 'Add column right', icon: ColumnsPlusRightIcon, run: () => chain().addColumnAfter().run() },
      { label: 'Delete column', icon: MinusSquareIcon, run: () => chain().deleteColumn().run() },
    ],
    [
      { label: 'Merge cells', icon: ArrowsInLineHorizontalIcon, run: () => chain().mergeCells().run() },
      { label: 'Split cell', icon: ArrowsOutLineHorizontalIcon, run: () => chain().splitCell().run() },
    ],
    [{ label: 'Delete table', icon: TrashIcon, run: () => chain().deleteTable().run(), danger: true }],
  ]

  return (
    <div
      role="toolbar"
      aria-label="Table"
      className="flex shrink-0 items-center gap-0.5 border-b border-border bg-muted/45 px-5 py-1 @3xl:px-14"
    >
      <span className="mr-2 ml-2.5 flex items-center gap-1.5 text-[0.7rem] font-bold tracking-wider text-muted-foreground uppercase">
        <TableIcon weight="duotone" className="size-4 text-chart-2" aria-hidden />
        Table
      </span>
      {groups.map((group, i) => (
        <Fragment key={i}>
          {i > 0 && <span className="mx-1.5 h-4 w-px shrink-0 bg-border" aria-hidden />}
          {group.map(({ label, icon: ActionIcon, run, danger }) => (
            <button
              key={label}
              type="button"
              onClick={run}
              title={label}
              aria-label={label}
              className={cn(
                toggleVariants({ variant: 'tool' }),
                'inline-flex items-center justify-center px-0 [&_svg]:size-[18px]',
                danger && 'text-destructive hover:text-destructive',
              )}
            >
              <ActionIcon />
            </button>
          ))}
        </Fragment>
      ))}
    </div>
  )
}
