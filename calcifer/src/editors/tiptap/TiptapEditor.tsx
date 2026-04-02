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
import { Toolbar } from './components/Toolbar'
import './TiptapEditor.css'

export function TiptapEditor() {
  const editor = useEditor({
    extensions: [
      StarterKit,
      TextStyle,
      Color,
      Highlight.configure({ multicolor: true }),
      FontFamily,
      Subscript,
      Superscript,
      TextAlign.configure({ types: ['heading', 'paragraph', 'blockquote'] }),
      TaskList,
      TaskItem.configure({ nested: true }),
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
