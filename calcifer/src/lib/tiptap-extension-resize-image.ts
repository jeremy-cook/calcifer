import Image from '@tiptap/extension-image'
import type { Node as ProsemirrorNode } from '@tiptap/pm/model'

const MIN_WIDTH = 50

const HANDLE_POSITIONS = [
  { top: '-4px', left: '-4px', cursor: 'nwse-resize' },
  { top: '-4px', right: '-4px', cursor: 'nesw-resize' },
  { bottom: '-4px', left: '-4px', cursor: 'nesw-resize' },
  { bottom: '-4px', right: '-4px', cursor: 'nwse-resize' },
] as const

export const ResizableImage = Image.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      caption: {
        default: '',
        parseHTML: (el) => el.getAttribute('data-caption'),
        renderHTML: () => ({}),
      },
      width: {
        default: null,
        parseHTML: (el) => el.style.width,
        renderHTML: (attrs) => (attrs.width ? { style: `width: ${attrs.width}` } : {}),
      },
    }
  },

  addNodeView() {
    return ({ node, editor, getPos }) => {
      let currentNode: ProsemirrorNode = node
      let showCaption = !!(node.attrs.caption as string)

      const wrapper = document.createElement('div')
      wrapper.style.cssText = 'display: block; margin-block: 0.75em;'

      // figure shrinks to image width so caption aligns under the image
      const figure = document.createElement('figure')
      figure.style.cssText = 'display: inline-block; max-width: 100%; margin: 0;'

      const container = document.createElement('div')
      container.style.cssText = 'position: relative; display: block; line-height: 0;'
      if (node.attrs.width) figure.style.width = node.attrs.width

      const img = document.createElement('img')
      img.style.cssText = 'width: 100%; height: auto; display: block; border-radius: var(--radius-md);'
      if (node.attrs.src) img.src = node.attrs.src as string
      if (node.attrs.alt) img.alt = node.attrs.alt as string
      if (node.attrs.title) img.title = node.attrs.title as string

      container.appendChild(img)

      // ── Resize handles ──────────────────────────────────────────────────────
      const handles = HANDLE_POSITIONS.map((pos, index) => {
        const handle = document.createElement('div')
        const posStyles = Object.entries(pos)
          .map(([k, v]) => `${k}: ${v}`)
          .join('; ')
        handle.style.cssText = `position: absolute; width: 9px; height: 9px; background: white; border: 1.5px solid var(--border); border-radius: 50%; display: none; z-index: 10; ${posStyles};`

        let isResizing = false
        let startX = 0
        let startWidth = 0

        const onMouseMove = (e: MouseEvent) => {
          if (!isResizing) return
          const deltaX = index % 2 === 0 ? -(e.clientX - startX) : e.clientX - startX
          figure.style.width = Math.max(MIN_WIDTH, startWidth + deltaX) + 'px'
        }

        const onMouseUp = () => {
          if (!isResizing) return
          isResizing = false
          document.removeEventListener('mousemove', onMouseMove)
          document.removeEventListener('mouseup', onMouseUp)
          if (typeof getPos === 'function') {
            const pos = getPos()
            if (pos === undefined) return
            editor.view.dispatch(
              editor.view.state.tr.setNodeMarkup(pos, undefined, {
                ...currentNode.attrs,
                width: figure.style.width,
              }),
            )
          }
        }

        handle.addEventListener('mousedown', (e) => {
          e.preventDefault()
          isResizing = true
          startX = e.clientX
          startWidth = figure.offsetWidth
          document.addEventListener('mousemove', onMouseMove)
          document.addEventListener('mouseup', onMouseUp)
        })

        container.appendChild(handle)
        return handle
      })

      // ── Add Caption button (overlay on image) ───────────────────────────────
      const addCaptionBtn = document.createElement('button')
      addCaptionBtn.textContent = 'Add Caption'
      addCaptionBtn.style.cssText =
        'position: absolute; bottom: 16px; left: 50%; transform: translateX(-50%); ' +
        'background: var(--background); color: var(--foreground); ' +
        'border: 2px solid var(--primary); border-radius: var(--radius-md); ' +
        'padding: 10px 28px; font-size: 0.9375rem; font-weight: 600; ' +
        'cursor: pointer; display: none; z-index: 10; white-space: nowrap; ' +
        'box-shadow: 0 2px 8px rgba(0,0,0,0.25);'

      addCaptionBtn.addEventListener('mousedown', (e) => {
        e.preventDefault()
        e.stopPropagation()
      })

      addCaptionBtn.addEventListener('click', (e) => {
        e.preventDefault()
        e.stopPropagation()
        showCaption = true
        addCaptionBtn.style.display = 'none'
        captionInput.style.display = 'block'
        captionInput.focus()
      })

      container.appendChild(addCaptionBtn)

      // ── Caption input (below image, shown when showCaption = true and selected) ──
      const captionInput = document.createElement('input')
      captionInput.type = 'text'
      captionInput.placeholder = 'Enter a caption…'
      captionInput.style.cssText =
        'display: none; width: 100%; box-sizing: border-box; ' +
        'text-align: center; font-size: 0.8125em; font-style: italic; ' +
        'color: var(--foreground); ' +
        'background: color-mix(in oklch, var(--muted) 70%, transparent); ' +
        'border: none; border-top: 1px solid var(--border); outline: none; ' +
        'padding: 0.5rem 0.75rem; ' +
        'border-bottom-left-radius: var(--radius-md); ' +
        'border-bottom-right-radius: var(--radius-md);'

      captionInput.addEventListener('mousedown', (e) => e.stopPropagation())

      captionInput.addEventListener('keydown', (e) => {
        e.stopPropagation()
        if (e.key === 'Enter') {
          e.preventDefault()
          captionInput.blur()
        } else if (e.key === 'Escape') {
          captionInput.value = (currentNode.attrs.caption as string) ?? ''
          captionInput.blur()
        }
      })

      captionInput.addEventListener('blur', () => {
        const newValue = captionInput.value.trim()
        if (!newValue) {
          showCaption = false
        }
        if (newValue !== ((currentNode.attrs.caption as string) ?? '')) {
          if (typeof getPos === 'function') {
            const pos = getPos()
            if (pos !== undefined) {
              editor.view.dispatch(
                editor.view.state.tr.setNodeMarkup(pos, undefined, {
                  ...currentNode.attrs,
                  caption: newValue,
                }),
              )
            }
          }
        }
      })

      // ── Caption display paragraph (shown when not selected and caption exists) ──
      const captionDisplay = document.createElement('p')
      captionDisplay.style.cssText =
        'display: none; text-align: center; font-size: 0.8125em; font-style: italic; ' +
        'color: var(--muted-foreground); margin-top: 0.25rem; cursor: text;'

      captionDisplay.addEventListener('click', () => {
        // Clicking caption text when node is selected will open the input
        // (selectNode will have already been called since this is inside the node view)
        showCaption = true
        captionDisplay.style.display = 'none'
        captionInput.style.display = 'block'
        captionInput.focus()
      })

      const syncCaption = (caption: string) => {
        captionDisplay.textContent = caption
        captionDisplay.style.display = caption ? 'block' : 'none'
        captionInput.value = caption
      }

      syncCaption((node.attrs.caption as string) ?? '')

      figure.appendChild(container)
      figure.appendChild(captionInput)
      figure.appendChild(captionDisplay)
      wrapper.appendChild(figure)

      return {
        dom: wrapper,

        selectNode() {
          handles.forEach((h) => (h.style.display = 'block'))
          container.style.outline = '2px solid var(--primary)'
          container.style.outlineOffset = '2px'
          captionDisplay.style.display = 'none'

          if (showCaption) {
            captionInput.value = (currentNode.attrs.caption as string) ?? ''
            captionInput.style.display = 'block'
            addCaptionBtn.style.display = 'none'
          } else {
            captionInput.style.display = 'none'
            addCaptionBtn.style.display = 'block'
          }
        },

        deselectNode() {
          handles.forEach((h) => (h.style.display = 'none'))
          container.style.outline = ''
          container.style.outlineOffset = ''
          addCaptionBtn.style.display = 'none'
          captionInput.style.display = 'none'
          const caption = (currentNode.attrs.caption as string) ?? ''
          captionDisplay.textContent = caption
          captionDisplay.style.display = caption ? 'block' : 'none'
        },

        update(updatedNode) {
          if (updatedNode.type !== node.type) return false
          currentNode = updatedNode
          img.src = (updatedNode.attrs.src as string) ?? ''
          img.alt = (updatedNode.attrs.alt as string) ?? ''
          if (updatedNode.attrs.title) img.title = updatedNode.attrs.title as string
          if (updatedNode.attrs.width) figure.style.width = updatedNode.attrs.width as string
          const caption = (updatedNode.attrs.caption as string) ?? ''
          if (!caption) showCaption = false
          captionDisplay.textContent = caption
          if (captionInput.style.display === 'none') {
            captionDisplay.style.display = caption ? 'block' : 'none'
          }
          captionInput.value = caption
          return true
        },

        // Tell ProseMirror to ignore events that originate from our native inputs.
        // Without this, ProseMirror intercepts keydown events (even after stopPropagation)
        // and processes them as editor commands, deleting the selected node when typing.
        stopEvent(event: Event) {
          const target = event.target as HTMLElement | null
          return !!(target && (target === captionInput || target === addCaptionBtn))
        },

        // Ignore DOM mutations inside our NodeView so ProseMirror doesn't
        // try to reconcile them against the document state.
        ignoreMutation() {
          return true
        },

        destroy() {
          // mouseup listeners self-clean; nothing extra to remove
        },
      }
    }
  },
})
