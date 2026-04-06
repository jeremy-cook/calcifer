import { useCallback, useState, useEffect, useRef } from 'react'
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext'
import { useLexicalNodeSelection } from '@lexical/react/useLexicalNodeSelection'
import { mergeRegister } from '@lexical/utils'
import {
  $createParagraphNode,
  $getNodeByKey,
  CLICK_COMMAND,
  COMMAND_PRIORITY_LOW,
  DRAGSTART_COMMAND,
  type NodeKey,
} from 'lexical'
import { $isImageNode } from '../nodes/ImageNode'

interface Props {
  src: string
  altText: string
  width: number | 'inherit'
  caption: string
  nodeKey: NodeKey
}

type Direction = 'nw' | 'ne' | 'se' | 'sw'

interface DragState {
  isResizing: boolean
  startX: number
  startY: number
  startWidth: number
  startHeight: number
  currentWidth: number
  currentHeight: number
  ratio: number
  direction: Direction
}

const MIN_WIDTH = 100

export function ImageComponent({ src, altText, width, caption, nodeKey }: Props) {
  const [editor] = useLexicalComposerContext()
  const [isSelected, setSelected, clearSelection] = useLexicalNodeSelection(nodeKey)
  const [isResizing, setIsResizing] = useState(false)
  const [captionText, setCaptionText] = useState(caption)
  const imageRef = useRef<HTMLImageElement>(null)
  const captionInputRef = useRef<HTMLInputElement>(null)
  const originalCaptionRef = useRef(caption)

  const imgWidth = width === 'inherit' ? '100%' : `${width}px`
  const isFocused = isSelected || isResizing

  useEffect(() => {
    setCaptionText(caption)
    originalCaptionRef.current = caption
  }, [caption])

  // Register Lexical click + drag commands
  useEffect(() => {
    return mergeRegister(
      editor.registerCommand<MouseEvent>(
        CLICK_COMMAND,
        (event) => {
          if (isResizing) return true
          if (event.target === imageRef.current) {
            if (!event.shiftKey) clearSelection()
            setSelected(!event.shiftKey ? true : !isSelected)
            return true
          }
          return false
        },
        COMMAND_PRIORITY_LOW,
      ),
      editor.registerCommand<DragEvent>(
        DRAGSTART_COMMAND,
        (event) => {
          if (event.target === imageRef.current) {
            event.preventDefault()
            return true
          }
          return false
        },
        COMMAND_PRIORITY_LOW,
      ),
    )
  }, [editor, isResizing, isSelected, setSelected, clearSelection])

  const saveCaption = useCallback(() => {
    editor.update(() => {
      const node = $getNodeByKey(nodeKey)
      if ($isImageNode(node)) node.setCaption(captionText)
      const next = node?.getNextSibling()
      if (next) {
        next.selectStart()
      } else {
        const paragraph = $createParagraphNode()
        node?.insertAfter(paragraph)
        paragraph.select()
      }
    })
    editor.focus()
  }, [editor, nodeKey, captionText])

  // ── Drag-to-resize ──────────────────────────────────────────────────────────
  const dragState = useRef<DragState>({
    isResizing: false,
    startX: 0,
    startY: 0,
    startWidth: 0,
    startHeight: 0,
    currentWidth: 0,
    currentHeight: 0,
    ratio: 1,
    direction: 'se',
  })

  const setResizeCursor = useCallback((cursor: string) => {
    document.body.style.cursor = cursor
    const editorRoot = document.querySelector('.lexical-content-editable') as HTMLElement | null
    if (editorRoot) editorRoot.style.cursor = cursor
  }, [])

  const clearResizeCursor = useCallback(() => {
    document.body.style.cursor = ''
    const editorRoot = document.querySelector('.lexical-content-editable') as HTMLElement | null
    if (editorRoot) editorRoot.style.cursor = ''
  }, [])

  const getMaxWidth = useCallback(() => {
    const container = document.querySelector('.lexical-content-editable') as HTMLElement | null
    return container ? container.offsetWidth - 128 : 600
  }, [])

  const onPointerDown = useCallback(
    (e: React.PointerEvent, direction: Direction) => {
      e.preventDefault()
      e.stopPropagation()

      const img = imageRef.current
      if (!img) return
      const rect = img.getBoundingClientRect()

      const ds = dragState.current
      ds.isResizing = true
      ds.startX = e.clientX
      ds.startY = e.clientY
      ds.startWidth = rect.width
      ds.startHeight = rect.height
      ds.currentWidth = rect.width
      ds.currentHeight = rect.height
      ds.ratio = rect.height / rect.width
      ds.direction = direction

      document.body.style.webkitUserSelect = 'none'
      ;(document.body.style as unknown as Record<string, string>)['userSelect'] = 'none'

      const cursorMap: Record<Direction, string> = {
        nw: 'nw-resize',
        ne: 'ne-resize',
        se: 'se-resize',
        sw: 'sw-resize',
      }
      setResizeCursor(cursorMap[direction])

      const onPointerMove = (ev: PointerEvent) => {
        if (!ds.isResizing) return
        const maxWidth = getMaxWidth()
        const dx = ev.clientX - ds.startX
        let newWidth = ds.startWidth

        if (direction === 'se' || direction === 'ne') {
          newWidth = ds.startWidth + dx
        } else {
          // nw or sw — dragging left shrinks, right grows
          newWidth = ds.startWidth - dx
        }

        newWidth = Math.max(MIN_WIDTH, Math.min(maxWidth, newWidth))
        ds.currentWidth = newWidth
        ds.currentHeight = newWidth * ds.ratio

        if (img) {
          img.style.width = `${newWidth}px`
          img.style.height = 'auto'
        }
      }

      const onPointerUp = () => {
        if (!ds.isResizing) return
        ds.isResizing = false

        document.body.style.webkitUserSelect = ''
        ;(document.body.style as unknown as Record<string, string>)['userSelect'] = ''
        clearResizeCursor()

        const newWidth = Math.round(ds.currentWidth)
        editor.update(() => {
          const node = $getNodeByKey(nodeKey)
          if ($isImageNode(node)) node.setWidth(newWidth)
        })

        document.removeEventListener('pointermove', onPointerMove)
        document.removeEventListener('pointerup', onPointerUp)

        setTimeout(() => setIsResizing(false), 200)
      }

      setIsResizing(true)
      document.addEventListener('pointermove', onPointerMove)
      document.addEventListener('pointerup', onPointerUp)
    },
    [editor, nodeKey, setResizeCursor, clearResizeCursor, getMaxWidth],
  )

  const handleStyle: React.CSSProperties = {
    position: 'absolute',
    width: 10,
    height: 10,
    background: 'white',
    border: '2px solid hsl(var(--primary))',
    borderRadius: '50%',
    zIndex: 10,
  }

  const corners: { dir: Direction; style: React.CSSProperties }[] = [
    { dir: 'nw', style: { top: -5, left: -5, cursor: 'nw-resize' } },
    { dir: 'ne', style: { top: -5, right: -5, cursor: 'ne-resize' } },
    { dir: 'se', style: { bottom: -5, right: -5, cursor: 'se-resize' } },
    { dir: 'sw', style: { bottom: -5, left: -5, cursor: 'sw-resize' } },
  ]

  return (
    <figure className={`my-3 inline-block max-w-full ${isFocused ? 'ring-2 ring-primary ring-offset-1' : ''}`} style={{ margin: '0.75em 0' }}>
      {/* Inner wrapper that shrinks to image width so handles sit at image corners */}
      <div style={{ display: 'inline-block', position: 'relative', maxWidth: '100%' }}>
        <img
          ref={imageRef}
          src={src}
          alt={altText}
          style={{ width: imgWidth, height: 'auto', display: 'block', borderRadius: 'var(--radius-md)' }}
          draggable={false}
        />
        {isFocused &&
          corners.map(({ dir, style }) => (
            <div
              key={dir}
              style={{ ...handleStyle, ...style }}
              onPointerDown={(e) => onPointerDown(e, dir)}
            />
          ))}
      </div>

      {/* Caption */}
      {isSelected ? (
        <input
          ref={captionInputRef}
          value={captionText}
          onChange={(e) => setCaptionText(e.target.value)}
          onBlur={saveCaption}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault()
              saveCaption()
            }
            if (e.key === 'Escape') {
              e.preventDefault()
              setCaptionText(originalCaptionRef.current)
              captionInputRef.current?.blur()
            }
          }}
          placeholder="Add caption…"
          className="mt-1 w-full border-0 bg-transparent px-2 py-0.5 text-center text-xs italic text-muted-foreground outline-none focus:border-b focus:border-border"
        />
      ) : captionText ? (
        <p className="mt-1 text-center text-xs italic text-muted-foreground">{captionText}</p>
      ) : null}
    </figure>
  )
}
