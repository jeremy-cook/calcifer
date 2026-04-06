import { useState, useRef } from 'react'
import { BubbleMenu } from '@tiptap/react/menus'
import type { Editor } from '@tiptap/react'
import { getMarkRange } from '@tiptap/core'
import { LinkEditorPopoverContent } from './LinkEditorPopoverContent'

interface LinkPopoverProps {
  editor: Editor
}

function getLinkText(editor: Editor): string {
  const { state } = editor
  const range = getMarkRange(state.selection.$from, state.schema.marks.link)
  if (!range) return ''
  return state.doc.textBetween(range.from, range.to)
}

export function LinkPopover({ editor }: LinkPopoverProps) {
  const [isEditing, setIsEditing] = useState(false)
  const [editedUrl, setEditedUrl] = useState('')
  const [editedLabel, setEditedLabel] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)

  const linkUrl = editor.getAttributes('link').href as string | undefined

  const handleEdit = () => {
    const { state } = editor
    const markType = state.schema.marks.link
    let href = ''
    state.selection.$from.marks().forEach((mark) => {
      if (mark.type === markType) href = mark.attrs.href as string
    })
    setEditedUrl(href)
    setEditedLabel(getLinkText(editor))
    setIsEditing(true)
    setTimeout(() => inputRef.current?.focus(), 0)
  }

  const handleSave = () => {
    if (editedUrl) {
      const label = editedLabel.trim() || editedUrl
      editor
        .chain()
        .focus()
        .extendMarkRange('link')
        .command(({ tr, state }) => {
          const { from, to } = state.selection
          const linkMark = state.schema.marks.link.create({
            href: editedUrl,
            target: '_blank',
            rel: 'noopener noreferrer',
          })
          tr.replaceWith(from, to, state.schema.text(label, [linkMark]))
          return true
        })
        .run()
    }
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
      options={{ placement: 'bottom', onHide: () => setIsEditing(false) }}
      className="flex items-center gap-1 rounded-md border border-border bg-popover px-2 py-1.5 shadow-md text-sm"
    >
      <LinkEditorPopoverContent
        linkUrl={linkUrl ?? ''}
        editedUrl={editedUrl}
        editedLabel={editedLabel}
        isEditing={isEditing}
        inputRef={inputRef}
        onEditedUrlChange={setEditedUrl}
        onEditedLabelChange={setEditedLabel}
        onSave={handleSave}
        onEdit={handleEdit}
        onOpen={() => window.open(linkUrl, '_blank', 'noopener,noreferrer')}
        onRemove={handleRemove}
        onEscape={() => setIsEditing(false)}
      />
    </BubbleMenu>
  )
}
