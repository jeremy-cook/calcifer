import { useEffect } from 'react'
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext'
import { TextNode } from 'lexical'

/**
 * Enforces that inline code cannot coexist with other text formats.
 * Uses a node transform so it fires at the data layer regardless of
 * how the format was applied (toolbar, keyboard shortcut, paste, etc.).
 */
export function CodeExclusivityPlugin() {
  const [editor] = useLexicalComposerContext()

  useEffect(() => {
    return editor.registerNodeTransform(TextNode, (node) => {
      if (!node.hasFormat('code')) return
      if (node.hasFormat('bold')) node.toggleFormat('bold')
      if (node.hasFormat('italic')) node.toggleFormat('italic')
      if (node.hasFormat('underline')) node.toggleFormat('underline')
      if (node.hasFormat('strikethrough')) node.toggleFormat('strikethrough')
    })
  }, [editor])

  return null
}
