import type { RefObject } from 'react'
import { Button } from '~/components/ui/button'
import { ArrowSquareOutIcon, PencilSimpleIcon, LinkBreakIcon, CheckIcon } from '@phosphor-icons/react'

interface Props {
  linkUrl: string
  editedUrl: string
  isEditing: boolean
  inputRef: RefObject<HTMLInputElement | null>
  onEditedUrlChange: (url: string) => void
  onSave: () => void
  onEdit: () => void
  onOpen: () => void
  onRemove: () => void
  onEscape: () => void
}

export function LinkEditorPopoverContent({
  linkUrl,
  editedUrl,
  isEditing,
  inputRef,
  onEditedUrlChange,
  onSave,
  onEdit,
  onOpen,
  onRemove,
  onEscape,
}: Props) {
  if (isEditing) {
    return (
      <>
        <input
          ref={inputRef}
          value={editedUrl}
          onChange={(e) => onEditedUrlChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') onSave()
            if (e.key === 'Escape') onEscape()
          }}
          className="w-64 rounded border border-border bg-background px-2 py-0.5 text-xs outline-none focus:border-primary"
          placeholder="https://"
        />
        <Button variant="ghost" size="sm" className="h-6 w-6 p-0" onClick={onSave} aria-label="Save">
          <CheckIcon size={14} />
        </Button>
      </>
    )
  }

  return (
    <>
      <a
        href={linkUrl}
        target="_blank"
        rel="noopener noreferrer"
        className="max-w-48 truncate text-primary underline text-xs"
      >
        {linkUrl}
      </a>
      <Button variant="ghost" size="sm" className="h-6 w-6 p-0" onClick={onOpen} aria-label="Open link">
        <ArrowSquareOutIcon size={14} />
      </Button>
      <Button variant="ghost" size="sm" className="h-6 w-6 p-0" onClick={onEdit} aria-label="Edit link">
        <PencilSimpleIcon size={14} />
      </Button>
      <Button variant="ghost" size="sm" className="h-6 w-6 p-0" onClick={onRemove} aria-label="Remove link">
        <LinkBreakIcon size={14} />
      </Button>
    </>
  )
}
