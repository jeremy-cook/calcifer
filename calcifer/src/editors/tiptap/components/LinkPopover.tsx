import { useState, useRef } from 'react'
import { BubbleMenu } from '@tiptap/react/menus'
import type { Editor } from '@tiptap/react'
import { LinkEditorPopoverContent } from '~/editors/shared/LinkEditorPopoverContent'

interface LinkPopoverProps {
  editor: Editor
}

export function LinkPopover({ editor }: LinkPopoverProps) {
  const [isEditing, setIsEditing] = useState(false)
  const [editedUrl, setEditedUrl] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)

  const linkUrl = editor.getAttributes('link').href as string | undefined

  const handleEdit = () => {
    setEditedUrl(linkUrl ?? '')
    setIsEditing(true)
    setTimeout(() => inputRef.current?.focus(), 0)
  }

  const handleSave = () => {
    if (editedUrl) editor.chain().focus().setLink({ href: editedUrl }).run()
    setIsEditing(false)
  }

  const handleRemove = () => {
    editor.chain().focus().unsetLink().run()
    setIsEditing(false)
  }

  return (
    <BubbleMenu
      editor={editor}
      shouldShow={({ editor: e }) => e.isActive('link')}
      tippyOptions={{ placement: 'bottom', onHidden: () => setIsEditing(false) }}
      className="flex items-center gap-1 rounded-md border border-border bg-popover px-2 py-1.5 shadow-md text-sm"
    >
      <LinkEditorPopoverContent
        linkUrl={linkUrl ?? ''}
        editedUrl={editedUrl}
        isEditing={isEditing}
        inputRef={inputRef}
        onEditedUrlChange={setEditedUrl}
        onSave={handleSave}
        onEdit={handleEdit}
        onOpen={() => window.open(linkUrl, '_blank', 'noopener,noreferrer')}
        onRemove={handleRemove}
        onEscape={() => setIsEditing(false)}
      />
    </BubbleMenu>
  )
}
