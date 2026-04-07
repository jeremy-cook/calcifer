import { useState, useRef } from 'react'
import { type Editor } from '@tiptap/react'
import { Popover, PopoverContent, PopoverTrigger } from '~/components/ui/popover'
import { Button } from '~/components/ui/button'
import { PlusIcon, TableIcon, ImageIcon, CaretRightIcon } from '@phosphor-icons/react'
import { readFileAsDataURL } from '~/lib/tiptap-extension-resize-image'

type View = 'menu' | 'table' | 'image'

const MAX = 8

interface Props {
  editor: Editor
}

export function InsertPopover({ editor }: Props) {
  const [open, setOpen] = useState(false)
  const [view, setView] = useState<View>('menu')
  const [hovered, setHovered] = useState({ rows: 0, cols: 0 })
  const [url, setUrl] = useState('')
  const [file, setFile] = useState<{ src: string; name: string } | null>(null)
  const [altText, setAltText] = useState('')
  const fileRef = useRef<HTMLInputElement>(null)

  const imageSrc = file?.src ?? url.trim()

  const resetForm = () => {
    setView('menu')
    setUrl('')
    setFile(null)
    setAltText('')
  }

  const close = () => {
    setOpen(false)
  }

  const handleInsertImage = () => {
    if (!imageSrc) return
    editor.chain().focus().setImage({ src: imageSrc, alt: altText.trim() }).run()
    close()
  }

  return (
    <Popover open={open} onOpenChange={(o) => { if (o) { resetForm(); setOpen(true) } else { setOpen(false) } }}>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="sm" className="h-7 gap-1 px-2 text-xs" aria-label="Insert">
          <PlusIcon />
          Insert
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-auto p-1.5" align="start">
        {view === 'menu' && (
          <div className="flex flex-col gap-0.5">
            {(
              [
                { icon: TableIcon, label: 'Table' },
                { icon: ImageIcon, label: 'Image' },
                { icon: CaretRightIcon, label: 'Details' },
              ] as const
            ).map(({ icon: Icon, label }) => (
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
          </div>
        )}

        {view === 'table' && (
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
        )}

        {view === 'image' && (
          <div className="w-64 space-y-2 p-1.5">
            <p className="text-xs font-medium">Insert image</p>
            {file ? (
              <div className="flex items-center gap-2 rounded border border-border bg-muted px-2 py-1.5">
                <span className="flex-1 truncate text-xs">{file.name}</span>
                <button
                  onClick={() => setFile(null)}
                  className="leading-none text-muted-foreground hover:text-foreground"
                  aria-label="Remove file"
                >
                  ×
                </button>
              </div>
            ) : (
              <input
                type="url"
                placeholder="https://..."
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && handleInsertImage()}
                className="w-full rounded border border-border bg-background px-2 py-1 text-xs outline-none focus:border-primary"
              />
            )}
            <Button
              variant="outline"
              size="sm"
              className="h-7 w-full text-xs"
              onClick={() => fileRef.current?.click()}
            >
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
              onKeyDown={(e) => e.key === 'Enter' && handleInsertImage()}
              className="w-full rounded border border-border bg-background px-2 py-1 text-xs outline-none focus:border-primary"
              aria-label="Alt text"
            />
            <Button size="sm" className="h-7 w-full text-xs" onClick={handleInsertImage} disabled={!imageSrc}>
              Insert
            </Button>
          </div>
        )}
      </PopoverContent>
    </Popover>
  )
}
