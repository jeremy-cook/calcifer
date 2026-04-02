import './LexicalEditor.css'
import { LexicalComposer } from '@lexical/react/LexicalComposer'
import { RichTextPlugin } from '@lexical/react/LexicalRichTextPlugin'
import { ContentEditable } from '@lexical/react/LexicalContentEditable'
import { HistoryPlugin } from '@lexical/react/LexicalHistoryPlugin'
import { LexicalErrorBoundary } from '@lexical/react/LexicalErrorBoundary'
import { MarkdownShortcutPlugin } from '@lexical/react/LexicalMarkdownShortcutPlugin'
import { ListPlugin } from '@lexical/react/LexicalListPlugin'
import { CheckListPlugin } from '@lexical/react/LexicalCheckListPlugin'
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
import { Toolbar } from './components/Toolbar'
import { CodeExclusivityPlugin } from './plugins/CodeExclusivityPlugin'

const theme = {
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
  nodes: [HeadingNode, QuoteNode, ListNode, ListItemNode],
}

export function LexicalEditor() {
  return (
    <LexicalComposer initialConfig={initialConfig}>
      <div className="flex h-full w-full flex-col border border-border">
        <Toolbar />
        <div className="relative min-h-0 flex-1 overflow-y-auto">
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
        <MarkdownShortcutPlugin transformers={[HEADING, QUOTE, UNORDERED_LIST, ORDERED_LIST, CHECK_LIST, BOLD_STAR, BOLD_UNDERSCORE, ITALIC_STAR, ITALIC_UNDERSCORE, STRIKETHROUGH, INLINE_CODE]} />
        <CodeExclusivityPlugin />
        </div>
      </div>
    </LexicalComposer>
  )
}
