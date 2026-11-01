import { useState } from 'react'
import { type Editor } from '@tiptap/react'
import { Popover, PopoverContent, PopoverTrigger } from '~/components/ui/popover'
import { Button } from '~/components/ui/button'
import { PlusIcon, TableIcon, ImageIcon, CaretRightIcon, CheckIcon } from '@phosphor-icons/react'
import { cn } from '~/lib/utils'
import { insertDetails } from '../insert-details'
import { ImageForm } from '../image/ImageForm'

type View = 'menu' | 'table' | 'image'

const MAX = 8

export type InsertItem = {
  key: string
  label: string
  icon: React.ReactNode
  active?: boolean
  disabled?: boolean
  onClick: () => void
  colorPicker?: {
    value: string
    onChange: (color: string) => void
  }
}

export type InsertSection = {
  id: string
  items: InsertItem[]
}

interface InsertMenuProps {
  editor: Editor
  sections: InsertSection[]
  close: () => void
  setView: (v: View) => void
}

interface InsertSectionsProps {
  sections: InsertSection[]
}

function renderColorPickerItem(item: InsertItem) {
  return (
    <label
      key={item.key}
      className={cn(
        'flex w-full cursor-pointer items-center gap-2.5 rounded-[9px] px-2.5 py-1.5 text-sm hover:bg-primary/15',
        item.disabled && 'pointer-events-none opacity-40',
      )}
    >
      <span className="flex size-3.75 items-center justify-center text-muted-foreground [&_svg]:size-3.75">
        {item.icon}
      </span>
      {item.label}
      {item.colorPicker!.value && (
        <span
          className="ml-auto h-3 w-3 rounded-full border border-border"
          style={{ backgroundColor: item.colorPicker!.value }}
        />
      )}
      <input
        type="color"
        className="sr-only"
        value={item.colorPicker!.value || '#000000'}
        onChange={(e) => item.colorPicker!.onChange(e.target.value)}
      />
    </label>
  )
}

function renderButtonItem(item: InsertItem) {
  return (
    <button
      key={item.key}
      disabled={item.disabled}
      className={cn(
        'flex w-full cursor-pointer items-center gap-2.5 rounded-[9px] px-2.5 py-1.5 text-left text-sm hover:bg-primary/15 disabled:pointer-events-none disabled:opacity-40',
        item.active && 'bg-muted',
      )}
      onClick={item.onClick}
    >
      <span className="flex size-3.75 items-center justify-center text-muted-foreground [&_svg]:size-3.75">
        {item.icon}
      </span>
      <span>{item.label}</span>
      {item.active && <CheckIcon size={12} className="ml-auto text-foreground" />}
    </button>
  )
}

function InsertSections({ sections }: InsertSectionsProps) {
  if (sections.length === 0) return null

  return (
    <>
      <div className="-mx-1.5 my-1 h-px bg-border" />
      {sections.map((section) =>
        section.items.map((item) => (item.colorPicker ? renderColorPickerItem(item) : renderButtonItem(item))),
      )}
    </>
  )
}

function InsertMenu({ editor, sections, close, setView }: InsertMenuProps) {
  const menuItems = [
    { icon: TableIcon, label: 'Table' },
    { icon: ImageIcon, label: 'Image' },
    { icon: CaretRightIcon, label: 'Details' },
  ] as const
  return (
    <div className="flex flex-col gap-0.5">
      {menuItems.map(({ icon: Icon, label }) => (
        <button
          key={label}
          className="flex w-full cursor-pointer items-center gap-2.5 rounded-[9px] px-2.5 py-1.5 text-left text-sm hover:bg-primary/15"
          onClick={() => {
            if (label === 'Details') {
              insertDetails(editor)
              close()
            } else {
              setView(label.toLowerCase() as View)
            }
          }}
        >
          <Icon size={15} className="text-muted-foreground" />
          {label}
        </button>
      ))}
      <InsertSections sections={sections} />
    </div>
  )
}

interface TablePickerProps {
  editor: Editor
  close: () => void
}

function TablePicker({ editor, close }: TablePickerProps) {
  const [hovered, setHovered] = useState({ rows: 0, cols: 0 })

  return (
    <div className="p-0.5">
      <div className="mb-1.5 text-center text-xs text-muted-foreground">
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
              className={`h-5 w-5 cursor-pointer rounded-sm border ${isActive ? 'border-primary bg-primary/20' : 'border-muted-foreground/30 bg-muted'}`}
              onMouseEnter={() => setHovered({ rows: row, cols: col })}
              onMouseLeave={() => setHovered({ rows: 0, cols: 0 })}
              onClick={() => {
                editor.chain().focus().insertTable({ rows: row, cols: col, withHeaderRow: true }).run()
                close()
              }}
            />
          )
        })}
      </div>
    </div>
  )
}

interface InsertPopoverProps {
  editor: Editor
  sections?: InsertSection[]
}

export function InsertPopover({ editor, sections = [] }: InsertPopoverProps) {
  const [open, setOpen] = useState(false)
  const [view, setView] = useState<View>('menu')

  const close = () => setOpen(false)

  return (
    <Popover
      open={open}
      onOpenChange={(o) => {
        if (o) {
          setView('menu')
          setOpen(true)
        } else {
          setOpen(false)
        }
      }}
    >
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className="ml-auto h-8 gap-1.5 px-2.5 text-sm font-semibold"
          aria-label="Insert"
        >
          <PlusIcon />
          Insert
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-auto p-1.5" align="end">
        {view === 'menu' && <InsertMenu editor={editor} sections={sections} close={close} setView={setView} />}
        {view === 'table' && <TablePicker editor={editor} close={close} />}
        {view === 'image' && (
          <ImageForm
            onInsert={(src, alt) => {
              editor.chain().focus().setImage({ src, alt }).run()
              close()
            }}
          />
        )}
      </PopoverContent>
    </Popover>
  )
}
