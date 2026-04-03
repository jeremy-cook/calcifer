import { useState, useEffect } from 'react'
import { useEditor, EditorContent } from '@tiptap/react'
import { StarterKit } from '@tiptap/starter-kit'
import { Placeholder } from '@tiptap/extension-placeholder'
import { TextStyle } from '@tiptap/extension-text-style'
import { Color } from '@tiptap/extension-color'
import { Highlight } from '@tiptap/extension-highlight'
import { FontFamily } from '@tiptap/extension-font-family'
import { Subscript } from '@tiptap/extension-subscript'
import { Superscript } from '@tiptap/extension-superscript'
import { TextAlign } from '@tiptap/extension-text-align'
import { TaskList } from '@tiptap/extension-task-list'
import { TaskItem } from '@tiptap/extension-task-item'
import { ReactNodeViewRenderer } from '@tiptap/react'
import { bundledLanguages } from 'shiki'
import type { Highlighter, BundledLanguage } from 'shiki'

import { CodeBlockShiki } from '~/lib/tiptap-extension-code-block-shiki'
import { getShikiHighlighter } from '~/lib/shiki'
import { CODE_LANGUAGES } from '~/editors/shared/formatting-options'
import { Toolbar } from './components/Toolbar'
import { TiptapCodeBlock } from './components/TiptapCodeBlock'
import './TiptapEditor.css'

function TiptapEditor({ highlighter }: { highlighter: Highlighter }) {
  const editor = useEditor({
    extensions: [
      StarterKit.configure({ codeBlock: false }),
      TextStyle,
      Color,
      Highlight.configure({ multicolor: true }),
      FontFamily,
      Subscript,
      Superscript,
      TextAlign.configure({ types: ['heading', 'paragraph', 'blockquote'] }),
      TaskList,
      TaskItem.configure({ nested: true }),
      CodeBlockShiki.configure({
        highlighter,
        themes: {
          light: 'dracula',
          dark: 'dracula',
        },
        languages: CODE_LANGUAGES.map((l) => l.value).filter((v): v is BundledLanguage => v in bundledLanguages),
      }).extend({
        addNodeView() {
          return ReactNodeViewRenderer(TiptapCodeBlock)
        },
      }),
      Placeholder.configure({ placeholder: 'Start typing…' }),
    ],
    content: '',
    editorProps: {
      attributes: {
        class: 'outline-none min-h-full px-16 py-10 text-base leading-normal',
      },
    },
  })

  if (!editor) return null

  return (
    <div className="flex h-full w-full flex-col border border-border">
      <Toolbar editor={editor} />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <EditorContent editor={editor} className="h-full" />
      </div>
    </div>
  )
}

export function TiptapEditorLoader() {
  const [highlighter, setHighlighter] = useState<Highlighter | null>(null)

  useEffect(() => {
    getShikiHighlighter().then(setHighlighter)
  }, [])

  if (!highlighter) return null

  return <TiptapEditor highlighter={highlighter} />
}
