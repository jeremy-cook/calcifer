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
import { Link } from '@tiptap/extension-link'
import { bundledLanguages } from 'shiki'
import type { BundledLanguage } from 'shiki'

import { Table } from '@tiptap/extension-table'
import { TableRow } from '@tiptap/extension-table-row'
import { TableHeader } from '@tiptap/extension-table-header'
import { TableCell } from '@tiptap/extension-table-cell'

import { CodeBlockShiki } from '~/lib/tiptap-extension-code-block-shiki'
import { ResizableImage } from '~/lib/tiptap-extension-resize-image'
import { readFileAsDataURL } from '~/lib/utils'
import { CODE_LANGUAGES } from '~/editors/shared/formatting-options'
import { Toolbar } from './components/Toolbar'
import { TiptapCodeBlock } from './components/TiptapCodeBlock'
import { LinkPopover } from './components/LinkPopover'
import { TableActionMenu } from './components/TableActionMenu'
import './TiptapEditor.css'


export function TiptapEditor() {
  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        codeBlock: false,
        link: false,
      }),
      TextStyle,
      Color,
      Highlight.configure({ multicolor: true }),
      FontFamily,
      Subscript,
      Superscript,
      TextAlign.configure({
        types: ['heading', 'paragraph', 'blockquote'],
      }),
      TaskList,
      TaskItem.configure({ nested: true }),
      CodeBlockShiki.configure({
        themes: {
          light: 'everforest-dark',
          dark: 'everforest-dark',
        },
        languages: CODE_LANGUAGES.map((l) => l.value).filter((v): v is BundledLanguage => v in bundledLanguages),
      }).extend({
        addNodeView() {
          return ReactNodeViewRenderer(TiptapCodeBlock)
        },
      }),
      Placeholder.configure({ placeholder: 'Start typing…' }),
      Link.configure({
        openOnClick: false,
        autolink: true,
        HTMLAttributes: { rel: 'noopener noreferrer', target: '_blank' },
      }),
      Table.configure({ resizable: false }),
      TableRow,
      TableHeader,
      TableCell,
      ResizableImage.configure({ inline: false, allowBase64: true }),
    ],
    content: '',
    editorProps: {
      attributes: {
        class: 'outline-none min-h-full px-16 py-10 text-base leading-normal',
      },
      handlePaste(view, event) {
        const items = Array.from(event.clipboardData?.items ?? [])
        const imageItem = items.find((item) => item.type.startsWith('image/'))
        if (imageItem) {
          event.preventDefault()
          const file = imageItem.getAsFile()
          if (file) {
            readFileAsDataURL(file).then((src) => {
              view.dispatch(view.state.tr.replaceSelectionWith(view.state.schema.nodes.image.create({ src })))
            })
          }
          return true
        }
        return false
      },
    },
  })

  if (!editor) return null

  return (
    <div className="flex h-full w-full flex-col border border-border">
      <Toolbar editor={editor} />
      <TableActionMenu editor={editor} />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <EditorContent editor={editor} className="h-full" />
      </div>
      <LinkPopover editor={editor} />
    </div>
  )
}
