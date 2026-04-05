import './LexicalEditor.css'
import { useEffect, useRef } from 'react'
import { LexicalComposer } from '@lexical/react/LexicalComposer'
import { RichTextPlugin } from '@lexical/react/LexicalRichTextPlugin'
import { ContentEditable } from '@lexical/react/LexicalContentEditable'
import { HistoryPlugin } from '@lexical/react/LexicalHistoryPlugin'
import { LexicalErrorBoundary } from '@lexical/react/LexicalErrorBoundary'
import { MarkdownShortcutPlugin } from '@lexical/react/LexicalMarkdownShortcutPlugin'
import { ListPlugin } from '@lexical/react/LexicalListPlugin'
import { CheckListPlugin } from '@lexical/react/LexicalCheckListPlugin'
import { TabIndentationPlugin } from '@lexical/react/LexicalTabIndentationPlugin'
import { LinkPlugin } from '@lexical/react/LexicalLinkPlugin'
import { AutoLinkPlugin } from '@lexical/react/LexicalAutoLinkPlugin'
import {
  BOLD_STAR,
  BOLD_UNDERSCORE,
  HEADING,
  INLINE_CODE,
  ITALIC_STAR,
  ITALIC_UNDERSCORE,
  QUOTE,
  STRIKETHROUGH,
  UNORDERED_LIST,
  ORDERED_LIST,
  CHECK_LIST,
} from '@lexical/markdown'
import { HeadingNode, QuoteNode } from '@lexical/rich-text'
import { ListNode, ListItemNode } from '@lexical/list'
import { CodeNode, CodeHighlightNode } from '@lexical/code'
import { LinkNode, AutoLinkNode, type LinkMatcher } from '@lexical/link'
import { TableNode, TableCellNode, TableRowNode } from '@lexical/table'
import { TablePlugin } from '@lexical/react/LexicalTablePlugin'
import { registerCodeHighlighting } from '@lexical/code-shiki'
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext'
import { Toolbar } from './components/Toolbar'
import { CodeExclusivityPlugin } from './plugins/CodeExclusivityPlugin'
import { CodeActionMenuPlugin } from './plugins/CodeActionMenuPlugin'
import { FloatingLinkEditorPlugin } from './plugins/FloatingLinkEditorPlugin'
import { TableActionMenuPlugin } from './plugins/TableActionMenuPlugin'
import { ImagesPlugin } from './plugins/ImagesPlugin'
import { ImageNode } from './nodes/ImageNode'

const URL_MATCHERS: Array<LinkMatcher> = [
  (text: string) => {
    const match = /((https?:\/\/(www\.)?)|(www\.))[-a-zA-Z0-9@:%._+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b([-a-zA-Z0-9()@:%_+.~#?&//=]*)/.exec(text)
    if (match === null) return null
    const fullMatch = match[0]
    return {
      index: match.index,
      length: fullMatch.length,
      text: fullMatch,
      url: fullMatch.startsWith('http') ? fullMatch : `https://${fullMatch}`,
    }
  },
  (text: string) => {
    const match = /(([^<>()[\]\\.,;:\s@"]+(\.[^<>()[\]\\.,;:\s@"]+)*)|(".+"))@((\[[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\])|(([a-zA-Z\-0-9]+\.)+[a-zA-Z]{2,}))/.exec(text)
    if (match === null) return null
    return { index: match.index, length: match[0].length, text: match[0], url: `mailto:${match[0]}` }
  },
]

const theme = {
  code: 'lexical-codeblock',
  link: 'lexical-link',
  table: 'lexical-table',
  tableRow: 'lexical-table-row',
  tableCell: 'lexical-table-cell',
  tableCellHeader: 'lexical-table-cell-header',
  text: {
    bold: 'font-bold',
    italic: 'italic',
    underline: 'underline',
    strikethrough: 'line-through',
    underlineStrikethrough: 'underline line-through',
    code: 'font-mono rounded bg-primary/10 text-primary px-1 py-0.5 text-sm',
  },
  heading: {
    h1: 'lexical-h1',
    h2: 'lexical-h2',
    h3: 'lexical-h3',
  },
  quote: 'lexical-blockquote',
  list: {
    ul: 'lexical-ul',
    ol: 'lexical-ol',
    listitem: 'lexical-listitem',
    listitemChecked: 'lexical-listitem-checked',
    listitemUnchecked: 'lexical-listitem-unchecked',
    nested: {
      listitem: 'lexical-nested-listitem',
    },
  },
}

function onError(error: Error) {
  console.error(error)
}

const initialConfig = {
  namespace: 'CalciferLexical',
  theme: theme,
  onError: onError,
  nodes: [HeadingNode, QuoteNode, ListNode, ListItemNode, CodeNode, CodeHighlightNode, LinkNode, AutoLinkNode, TableNode, TableCellNode, TableRowNode, ImageNode],
}

function CodeHighlightPlugin() {
  const [editor] = useLexicalComposerContext()
  useEffect(() => registerCodeHighlighting(editor), [editor])
  return null
}

export function LexicalEditor() {
  const scrollContainerRef = useRef<HTMLDivElement>(null)

  return (
    <LexicalComposer initialConfig={initialConfig}>
      <div className="flex h-full w-full flex-col border border-border">
        <Toolbar />
        <TableActionMenuPlugin />
        <div ref={scrollContainerRef} className="relative min-h-0 flex-1 overflow-y-auto">
          <RichTextPlugin
            contentEditable={
              <ContentEditable className="lexical-content-editable outline-none min-h-full px-16 py-10 text-base leading-normal" />
            }
            placeholder={
              <div className="pointer-events-none absolute left-16 top-10 text-muted-foreground">
                Start typing…
              </div>
            }
            ErrorBoundary={LexicalErrorBoundary}
          />
          <HistoryPlugin />
          <ListPlugin />
          <CheckListPlugin />
          <TabIndentationPlugin />
          <CodeHighlightPlugin />
          <MarkdownShortcutPlugin transformers={[HEADING, QUOTE, UNORDERED_LIST, ORDERED_LIST, CHECK_LIST, BOLD_STAR, BOLD_UNDERSCORE, ITALIC_STAR, ITALIC_UNDERSCORE, STRIKETHROUGH, INLINE_CODE]} />
          <CodeExclusivityPlugin />
          <CodeActionMenuPlugin scrollContainerRef={scrollContainerRef} />
          <LinkPlugin validateUrl={(url) => /^https?:\/\//.test(url) || url.startsWith('/')} />
          <AutoLinkPlugin matchers={URL_MATCHERS} />
          <FloatingLinkEditorPlugin />
          <TablePlugin hasCellMerge={true} hasCellBackgroundColor={true} hasTabHandler={true} />
          <ImagesPlugin />
        </div>
      </div>
    </LexicalComposer>
  )
}
