import type { RefObject } from 'react'
import { Button } from '~/components/ui/button'
import { ArrowSquareOutIcon, PencilSimpleIcon, LinkBreakIcon } from '@phosphor-icons/react'
import { LinkFields } from './LinkFields'

interface Props {
  linkUrl: string
  editedUrl: string
  editedLabel: string
  isEditing: boolean
  inputRef: RefObject<HTMLInputElement | null>
  onEditedUrlChange: (url: string) => void
  onEditedLabelChange: (label: string) => void
  onSave: () => void
  onEdit: () => void
  onOpen: () => void
  onRemove: () => void
  onEscape: () => void
}

export function LinkEditorPopover({
  linkUrl,
  editedUrl,
  editedLabel,
  isEditing,
  inputRef,
  onEditedUrlChange,
  onEditedLabelChange,
  onSave,
  onEdit,
  onOpen,
  onRemove,
  onEscape,
}: Props) {
  if (isEditing) {
    return (
      <LinkFields
        label={editedLabel}
        url={editedUrl}
        onLabelChange={onEditedLabelChange}
        onUrlChange={onEditedUrlChange}
        onSave={onSave}
        onEscape={onEscape}
        urlRef={inputRef}
      />
    )
  }

  return (
    <>
      <a
        href={linkUrl}
        target="_blank"
        rel="noopener noreferrer"
        className="max-w-56 truncate px-1 text-primary underline"
      >
        {linkUrl}
      </a>
      <Button variant="ghost" size="icon-sm" onClick={onOpen} aria-label="Open link">
        <ArrowSquareOutIcon />
      </Button>
      <Button variant="ghost" size="icon-sm" onClick={onEdit} aria-label="Edit link">
        <PencilSimpleIcon />
      </Button>
      <Button variant="ghost" size="icon-sm" onClick={onRemove} aria-label="Remove link">
        <LinkBreakIcon />
      </Button>
    </>
  )
}
