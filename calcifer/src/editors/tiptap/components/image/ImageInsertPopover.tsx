import { useState, useRef } from 'react'
import { type Editor } from '@tiptap/react'
import { Popover, PopoverContent, PopoverTrigger } from '~/components/ui/popover'
import { Button } from '~/components/ui/button'
import { ImageIcon } from '@phosphor-icons/react'
import { readFileAsDataURL } from '~/lib/tiptap-extension-resize-image'

interface ImageInsertPopoverProps {
  editor: Editor
}

export function ImageInsertPopover({ editor }: ImageInsertPopoverProps) {
  const [url, setUrl] = useState('')
  const [file, setFile] = useState<{ src: string; name: string } | null>(null)
  const [altText, setAltText] = useState('')
  const [open, setOpen] = useState(false)
  const fileRef = useRef<HTMLInputElement>(null)

  const src = file?.src ?? url.trim()

  const reset = () => {
    setUrl('')
    setFile(null)
    setAltText('')
  }

  const handleInsert = () => {
    if (!src) return
    editor.chain().focus().setImage({ src, alt: altText.trim() }).run()
    reset()
    setOpen(false)
  }

  const handleFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0]
    if (!f) return
    setFile({ src: await readFileAsDataURL(f), name: f.name })
    setUrl('')
    e.target.value = ''
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
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="sm" className="h-7 min-w-7 px-1.5" aria-label="Insert image">
          <ImageIcon />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-72 p-3" align="start">
        <div className="space-y-2">
          <p className="text-xs font-medium">Insert image</p>

          {file ? renderFilePreview() : renderUrlInput()}

          <Button variant="outline" size="sm" className="h-7 w-full text-xs" onClick={() => fileRef.current?.click()}>
            Upload file
          </Button>
          <input ref={fileRef} type="file" accept="image/*" className="hidden" onChange={handleFile} />

          <input
            type="text"
            placeholder="Describe the image…"
            value={altText}
            onChange={(e) => setAltText(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleInsert()}
            className="w-full rounded border border-border bg-background px-2 py-1 text-xs outline-none focus:border-primary"
            aria-label="Alt text"
          />

          <Button size="sm" className="h-7 w-full text-xs" onClick={handleInsert} disabled={!src}>
            Insert
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  )
}
