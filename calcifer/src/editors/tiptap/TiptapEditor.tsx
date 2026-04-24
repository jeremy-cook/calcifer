import { useEditor, EditorContent } from '@tiptap/react'
import type { Content, JSONContent } from '@tiptap/core'
import { DragHandle as DragHandleReact } from '@tiptap/extension-drag-handle-react'
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
import { Typography } from '@tiptap/extension-typography'
import { ReactNodeViewRenderer } from '@tiptap/react'
import { Link } from '@tiptap/extension-link'
import { bundledLanguages } from 'shiki'
import type { BundledLanguage } from 'shiki'

import { Details, DetailsContent, DetailsSummary } from '@tiptap/extension-details'
import { Table } from '@tiptap/extension-table'
import { TableRow } from '@tiptap/extension-table-row'
import { TableHeader } from '@tiptap/extension-table-header'
import { TableCell } from '@tiptap/extension-table-cell'

import { CodeBlockShiki } from '~/lib/tiptap-extension-code-block-shiki'
import { DateChip } from '~/lib/tiptap-extension-date'
import { DateChipView } from '~/editors/tiptap/components/date/DateChipView'
import { ResizableImage } from '~/lib/tiptap-extension-resize-image'
import { CODE_LANGUAGES } from './components/formattingOptions'
import { SlashCommand } from '~/lib/tiptap-extension-slash-command'
import { slashSuggestion } from './components/slash-menu/slashSuggestion'
import { EntityMention, HashtagMention } from './extensions/entityMention'
import { makeSuggestion } from './components/mention/makeSuggestion'
import { Toolbar } from './components/toolbar/Toolbar'
import { TiptapCodeBlock } from './components/code-block/TiptapCodeBlock'
import { DotsSixVerticalIcon } from '@phosphor-icons/react'
import { LinkActionMenu } from './components/link/LinkActionMenu'
import { TableActionMenu } from './components/table/TableActionMenu'
import { ImageActionMenu } from './components/image/ImageActionMenu'

interface TiptapEditorProps {
  doc?: string
  onUpdate?: (docJSON: JSONContent) => void
}

function parseDoc(doc: string | undefined): Content {
  if (!doc) return null
  return doc.length > 0 ? (JSON.parse(doc) as JSONContent) : null
}

export function TiptapEditor({ doc, onUpdate }: TiptapEditorProps) {
  const editor = useEditor({
    shouldRerenderOnTransaction: false,
    content: parseDoc(doc),
    onUpdate: ({ editor: e }) => onUpdate?.(e.getJSON()),
    extensions: [
      StarterKit.configure({
        codeBlock: false,
        link: false,
      }),
      Typography,
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
      Details.configure({
        renderToggleButton({ element, isOpen }) {
          element.setAttribute('contenteditable', 'false')
          element.innerHTML = isOpen ? '▾' : '▸'
        },
      }),
      DetailsSummary,
      DetailsContent,
      Table.configure({ resizable: false }),
      TableRow,
      TableHeader,
      TableCell,
      ResizableImage.configure({ inline: false, allowBase64: true }),
      DateChip.extend({
        addNodeView() {
          return ReactNodeViewRenderer(DateChipView)
        },
      }),
      SlashCommand.configure({ suggestion: slashSuggestion }),
      EntityMention.configure({
        HTMLAttributes: { class: 'mention' },
        suggestion: makeSuggestion({ char: '@' }),
      }),
      HashtagMention.configure({
        HTMLAttributes: { class: 'mention' },
        suggestion: makeSuggestion({ char: '#', structureFilter: 'Tag' }),
      }),
    ],
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
      <TableActionMenu editor={editor} />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <DragHandleReact editor={editor}>
          <DotsSixVerticalIcon weight="bold" />
        </DragHandleReact>
        <EditorContent editor={editor} className="h-full" />
      </div>
      <LinkActionMenu editor={editor} />
      <ImageActionMenu editor={editor} />
    </div>
  )
}
