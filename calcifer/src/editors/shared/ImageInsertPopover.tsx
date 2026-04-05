import { useState, useRef } from 'react'
import { Popover, PopoverContent, PopoverTrigger } from '~/components/ui/popover'
import { Button } from '~/components/ui/button'
import { ImageIcon } from '@phosphor-icons/react'
import { readFileAsDataURL } from '~/lib/utils'

interface Props {
  onInsert: (src: string) => void
}

export function ImageInsertPopover({ onInsert }: Props) {
  const [url, setUrl] = useState('')
  const [open, setOpen] = useState(false)
  const fileRef = useRef<HTMLInputElement>(null)

  const insertUrl = () => {
    if (url.trim()) {
      onInsert(url.trim())
      setUrl('')
      setOpen(false)
    }
  }

  const handleFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return
    const src = await readFileAsDataURL(file)
    onInsert(src)
    setOpen(false)
  }

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
          <div className="flex gap-2">
            <input
              type="url"
              placeholder="https://..."
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter') insertUrl() }}
              className="flex-1 rounded border border-border bg-background px-2 py-1 text-xs outline-none focus:border-primary"
            />
            <Button size="sm" className="h-7 text-xs" onClick={insertUrl}>Insert</Button>
          </div>
          <div>
            <Button variant="outline" size="sm" className="h-7 w-full text-xs" onClick={() => fileRef.current?.click()}>
              Upload file
            </Button>
            <input ref={fileRef} type="file" accept="image/*" className="hidden" onChange={handleFile} />
          </div>
        </div>
      </PopoverContent>
    </Popover>
  )
}
