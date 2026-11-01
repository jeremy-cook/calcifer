import { useRef, useState } from 'react'
import { ImageIcon, UploadSimpleIcon, XIcon } from '@phosphor-icons/react'
import { Button } from '~/components/ui/button'
import { Input } from '~/components/ui/input'
import { readFileAsDataURL } from '~/lib/tiptap-extension-resize-image'

export interface ImageFormProps {
  onInsert: (src: string, alt: string) => void
}

// An image by address or from a file, with a description for screen readers.
export function ImageForm({ onInsert }: ImageFormProps) {
  const [url, setUrl] = useState('')
  const [file, setFile] = useState<{ src: string; name: string } | null>(null)
  const [altText, setAltText] = useState('')
  const fileRef = useRef<HTMLInputElement>(null)

  const src = file?.src ?? url.trim()

  const handleInsert = () => {
    if (!src) return
    onInsert(src, altText.trim())
  }

  const handleFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0]
    if (!f) return
    setFile({ src: await readFileAsDataURL(f), name: f.name })
    setUrl('')
    e.target.value = ''
  }

  const renderFilePreview = () => (
    <div className="flex h-8 items-center gap-2 rounded-lg bg-muted pr-1 pl-2.5">
      <ImageIcon className="size-4 shrink-0 text-muted-foreground" aria-hidden />
      <span className="flex-1 truncate text-sm">{file!.name}</span>
      <Button variant="ghost" size="icon-xs" onClick={() => setFile(null)} aria-label="Remove file">
        <XIcon />
      </Button>
    </div>
  )

  const renderUrlInput = () => (
    <Input
      type="url"
      placeholder="https://"
      value={url}
      onChange={(e) => setUrl(e.target.value)}
      onKeyDown={(e) => e.key === 'Enter' && handleInsert()}
      aria-label="Image address"
    />
  )

  return (
    <div className="flex w-72 flex-col gap-2.5 p-1.5">
      <p className="font-heading text-base font-semibold text-heading">Insert image</p>
      <div className="flex items-center gap-1.5">
        <div className="min-w-0 flex-1">{file ? renderFilePreview() : renderUrlInput()}</div>
        <Button
          variant="outline"
          size="icon"
          onClick={() => fileRef.current?.click()}
          aria-label="Upload a file"
          title="Upload a file"
        >
          <UploadSimpleIcon />
        </Button>
      </div>
      <input ref={fileRef} type="file" accept="image/*" className="hidden" onChange={handleFile} />
      <label className="flex flex-col gap-1 text-xs font-semibold text-muted-foreground">
        Description
        <Input
          placeholder="For screen readers"
          value={altText}
          onChange={(e) => setAltText(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && handleInsert()}
          className="text-foreground"
        />
      </label>
      <Button className="w-full" onClick={handleInsert} disabled={!src}>
        Insert
      </Button>
    </div>
  )
}
