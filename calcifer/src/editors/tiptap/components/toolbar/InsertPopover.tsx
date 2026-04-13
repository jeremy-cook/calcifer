import { useState, useRef } from 'react'
import { type Editor } from '@tiptap/react'
import { Popover, PopoverContent, PopoverTrigger } from '~/components/ui/popover'
import { Button } from '~/components/ui/button'
import { PlusIcon, TableIcon, ImageIcon, CaretRightIcon, CheckIcon } from '@phosphor-icons/react'
import { readFileAsDataURL } from '~/lib/tiptap-extension-resize-image'
import { cn } from '~/lib/utils'

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
  fontFamily?: string
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
        'flex w-full cursor-pointer items-center gap-2.5 rounded px-2.5 py-1.5 text-sm hover:bg-accent',
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
        'flex w-full cursor-pointer items-center gap-2.5 rounded px-2.5 py-1.5 text-left text-sm hover:bg-accent disabled:pointer-events-none disabled:opacity-40',
        item.active && 'bg-muted',
      )}
      onClick={item.onClick}
    >
      <span className="flex size-3.75 items-center justify-center text-muted-foreground [&_svg]:size-3.75">
        {item.icon}
      </span>
      <span style={item.fontFamily ? { fontFamily: item.fontFamily } : undefined}>{item.label}</span>
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
        section.items.map((item) => (item.colorPicker ? renderColorPickerItem(item) : renderButtonItem(item)))
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
          className="flex w-full cursor-pointer items-center gap-2.5 rounded px-2.5 py-1.5 text-left text-sm hover:bg-accent"
          onClick={() => {
            if (label === 'Details') {
              editor.chain().focus().setDetails().run()
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
              className={`h-5 w-5 cursor-pointer rounded-sm border ${isActive ? 'border-primary bg-primary/20' : 'border-border bg-muted/30'}`}
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

interface ImageFormProps {
  onInsert: (src: string, alt: string) => void
}

function ImageForm({ onInsert }: ImageFormProps) {
  const [url, setUrl] = useState('')
  const [file, setFile] = useState<{ src: string; name: string } | null>(null)
  const [altText, setAltText] = useState('')
  const fileRef = useRef<HTMLInputElement>(null)

  const imageSrc = file?.src ?? url.trim()

  const handleInsert = () => {
    if (!imageSrc) return
    onInsert(imageSrc, altText.trim())
  }

  const renderFilePreview = () => (
    <div className="flex items-center gap-2 rounded border border-border bg-muted px-2 py-1.5">
      <span className="flex-1 truncate text-xs">{file!.name}</span>
      <button
        onClick={() => setFile(null)}
        className="leading-none text-muted-foreground hover:text-foreground"
        aria-label="Remove file"
      >
        ×
      </button>
    </div>
  )

  const renderUrlInput = () => (
    <input
      type="url"
      placeholder="https://..."
      value={url}
      onChange={(e) => setUrl(e.target.value)}
      onKeyDown={(e) => e.key === 'Enter' && handleInsert()}
      className="w-full rounded border border-border bg-background px-2 py-1 text-xs outline-none focus:border-primary"
    />
  )

  return (
    <div className="w-64 space-y-2 p-1.5">
      <p className="text-xs font-medium">Insert image</p>
      {file ? renderFilePreview() : renderUrlInput()}
      <Button variant="outline" size="sm" className="h-7 w-full text-xs" onClick={() => fileRef.current?.click()}>
        Upload file
      </Button>
      <input
        ref={fileRef}
        type="file"
        accept="image/*"
        className="hidden"
        onChange={async (e) => {
          const f = e.target.files?.[0]
          if (!f) return
          setFile({ src: await readFileAsDataURL(f), name: f.name })
          setUrl('')
          e.target.value = ''
        }}
      />
      <input
        type="text"
        placeholder="Describe the image…"
        value={altText}
        onChange={(e) => setAltText(e.target.value)}
        onKeyDown={(e) => e.key === 'Enter' && handleInsert()}
        className="w-full rounded border border-border bg-background px-2 py-1 text-xs outline-none focus:border-primary"
        aria-label="Alt text"
      />
      <Button size="sm" className="h-7 w-full text-xs" onClick={handleInsert} disabled={!imageSrc}>
        Insert
      </Button>
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
        <Button variant="ghost" size="sm" className="h-8 gap-1 px-2 text-xs" aria-label="Insert">
          <PlusIcon />
          Insert
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-auto p-1.5" align="start">
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
