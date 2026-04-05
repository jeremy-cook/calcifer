import { useCallback, useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext'
import { $getSelection, $isRangeSelection } from 'lexical'
import { $isLinkNode, $isAutoLinkNode, TOGGLE_LINK_COMMAND } from '@lexical/link'
import { $findMatchingParent, mergeRegister } from '@lexical/utils'
import { LinkEditorPopoverContent } from '~/editors/shared/LinkEditorPopoverContent'

export function FloatingLinkEditorPlugin() {
  const [editor] = useLexicalComposerContext()
  const [linkUrl, setLinkUrl] = useState('')
  const [editedUrl, setEditedUrl] = useState('')
  const [isEditing, setIsEditing] = useState(false)
  const [rect, setRect] = useState<DOMRect | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  const updateLinkEditor = useCallback(() => {
    const selection = $getSelection()
    if ($isRangeSelection(selection)) {
      const node = selection.anchor.getNode()
      const linkNode = $findMatchingParent(node, (n) => $isLinkNode(n) || $isAutoLinkNode(n))
      if (linkNode && ($isLinkNode(linkNode) || $isAutoLinkNode(linkNode))) {
        const url = linkNode.getURL()
        setLinkUrl((prev) => (prev === url ? prev : url))
        const domSelection = window.getSelection()
        if (domSelection && domSelection.rangeCount > 0) {
          const newRect = domSelection.getRangeAt(0).getBoundingClientRect()
          setRect((prev) =>
            prev && prev.top === newRect.top && prev.left === newRect.left ? prev : newRect,
          )
        }
        return
      }
    }
    setRect(null)
    setIsEditing(false)
  }, [])

  useEffect(() => {
    return mergeRegister(
      editor.registerUpdateListener(({ editorState }) => {
        editorState.read(() => updateLinkEditor())
      }),
    )
  }, [editor, updateLinkEditor])

  useEffect(() => {
    if (isEditing) setTimeout(() => inputRef.current?.focus(), 0)
  }, [isEditing])

  const handleEdit = () => {
    setEditedUrl(linkUrl)
    setIsEditing(true)
  }

  const handleSave = () => {
    if (editedUrl) {
      editor.dispatchCommand(TOGGLE_LINK_COMMAND, { url: editedUrl, target: '_blank', rel: 'noopener noreferrer' })
    }
    setIsEditing(false)
  }

  const handleRemove = () => {
    editor.dispatchCommand(TOGGLE_LINK_COMMAND, null)
    setRect(null)
  }

  const handleOpen = () => window.open(linkUrl, '_blank', 'noopener,noreferrer')

  if (!rect) return null

  return createPortal(
    <div
      className="fixed z-50 flex items-center gap-1 rounded-md border border-border bg-popover px-2 py-1.5 shadow-md text-sm"
      style={{ top: rect.bottom + 6, left: rect.left }}
    >
      <LinkEditorPopoverContent
        linkUrl={linkUrl}
        editedUrl={editedUrl}
        isEditing={isEditing}
        inputRef={inputRef}
        onEditedUrlChange={setEditedUrl}
        onSave={handleSave}
        onEdit={handleEdit}
        onOpen={handleOpen}
        onRemove={handleRemove}
        onEscape={() => setIsEditing(false)}
      />
    </div>,
    document.body,
  )
}
