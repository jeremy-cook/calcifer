import { useCallback, useState, useEffect } from 'react'
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext'
import { useLexicalNodeSelection } from '@lexical/react/useLexicalNodeSelection'
import { $getNodeByKey, type NodeKey } from 'lexical'
import { $isImageNode } from '../nodes/ImageNode'

interface Props {
  src: string
  altText: string
  width: number | 'inherit'
  caption: string
  nodeKey: NodeKey
}

export function ImageComponent({ src, altText, width, caption, nodeKey }: Props) {
  const [editor] = useLexicalComposerContext()
  const [isSelected, setIsSelected] = useLexicalNodeSelection(nodeKey)
  const [captionText, setCaptionText] = useState(caption)
  const [isEditingCaption, setIsEditingCaption] = useState(false)

  const imgWidth = width === 'inherit' ? '100%' : `${width}px`

  useEffect(() => {
    setCaptionText(caption)
  }, [caption])

  const saveCaption = useCallback(() => {
    editor.update(() => {
      const node = $getNodeByKey(nodeKey)
      if ($isImageNode(node)) node.setCaption(captionText)
    })
    setIsEditingCaption(false)
  }, [editor, nodeKey, captionText])

  const handleSizeChange = useCallback(
    (pct: number) => {
      const container = document.querySelector('.lexical-content-editable') as HTMLElement | null
      const containerWidth = container ? container.offsetWidth - 128 : 600
      editor.update(() => {
        const node = $getNodeByKey(nodeKey)
        if ($isImageNode(node)) node.setWidth(Math.round((containerWidth * pct) / 100))
      })
    },
    [editor, nodeKey],
  )

  return (
    <div className={`relative my-3 inline-block w-full ${isSelected ? 'ring-2 ring-primary ring-offset-1' : ''}`}>
      <img
        src={src}
        alt={altText}
        style={{ width: imgWidth, height: 'auto', display: 'block', borderRadius: 'var(--radius-md)' }}
        onClick={() => setIsSelected(true)}
        draggable={false}
      />
      {isSelected && (
        <div className="mt-1 flex items-center gap-1">
          <span className="text-xs text-muted-foreground">Size:</span>
          {[25, 50, 75, 100].map((pct) => (
            <button
              key={pct}
              className="rounded border border-border px-1.5 py-0.5 text-xs hover:bg-muted"
              onMouseDown={(e) => {
                e.preventDefault()
                handleSizeChange(pct)
              }}
            >
              {pct}%
            </button>
          ))}
        </div>
      )}
      {isEditingCaption ? (
        <input
          value={captionText}
          onChange={(e) => setCaptionText(e.target.value)}
          onBlur={saveCaption}
          onKeyDown={(e) => {
            if (e.key === 'Enter') saveCaption()
            if (e.key === 'Escape') setIsEditingCaption(false)
          }}
          placeholder="Add caption..."
          className="mt-1 w-full rounded border border-border bg-background px-2 py-0.5 text-center text-xs text-muted-foreground outline-none focus:border-primary"
          autoFocus
        />
      ) : (
        <p
          className="mt-1 cursor-text text-center text-xs italic text-muted-foreground"
          onClick={() => setIsEditingCaption(true)}
        >
          {captionText || 'Click to add caption…'}
        </p>
      )}
    </div>
  )
}
