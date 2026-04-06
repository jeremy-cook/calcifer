import { useState } from 'react'
import { type Editor } from '@tiptap/react'
import { Popover, PopoverContent, PopoverTrigger } from '~/components/ui/popover'
import { Button } from '~/components/ui/button'
import { TableIcon } from '@phosphor-icons/react'

const MAX = 8

interface Props { editor: Editor }

export function TableInsertPopover({ editor }: Props) {
  const [hovered, setHovered] = useState({ rows: 0, cols: 0 })
  const [open, setOpen] = useState(false)

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="sm" className="h-7 min-w-7 px-1.5" aria-label="Insert table">
          <TableIcon />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-auto p-2" align="start">
        <div className="mb-1 text-center text-xs text-muted-foreground">
          {hovered.rows > 0 ? `${hovered.rows} × ${hovered.cols}` : 'Insert table'}
        </div>
        <div className="grid gap-0.5" style={{ gridTemplateColumns: `repeat(${MAX}, 1.25rem)` }}>
          {Array.from({ length: MAX * MAX }).map((_, i) => {
            const row = Math.floor(i / MAX) + 1
            const col = (i % MAX) + 1
            const isActive = row <= hovered.rows && col <= hovered.cols
            return (
              <div
                key={i}
                className={`h-5 w-5 cursor-pointer rounded-sm border ${isActive ? 'border-primary bg-primary/20' : 'border-border bg-muted/30'}`}
                onMouseEnter={() => setHovered({ rows: row, cols: col })}
                onMouseLeave={() => setHovered({ rows: 0, cols: 0 })}
                onClick={() => {
                  editor.chain().focus().insertTable({ rows: row, cols: col, withHeaderRow: true }).run()
                  setOpen(false)
                }}
              />
            )
          })}
        </div>
      </PopoverContent>
    </Popover>
  )
}
