import './LexicalEditor.css'
import { LexicalComposer } from '@lexical/react/LexicalComposer'
import { RichTextPlugin } from '@lexical/react/LexicalRichTextPlugin'
import { ContentEditable } from '@lexical/react/LexicalContentEditable'
import { HistoryPlugin } from '@lexical/react/LexicalHistoryPlugin'
import { LexicalErrorBoundary } from '@lexical/react/LexicalErrorBoundary'
import { MarkdownShortcutPlugin } from '@lexical/react/LexicalMarkdownShortcutPlugin'
import { BOLD_STAR, BOLD_UNDERSCORE, INLINE_CODE, ITALIC_STAR, ITALIC_UNDERSCORE, STRIKETHROUGH } from '@lexical/markdown'
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
}

function onError(error: Error) {
  console.error(error)
}

const initialConfig = {
  namespace: 'CalciferLexical',
  theme: theme,
  onError: onError,
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
        <MarkdownShortcutPlugin transformers={[BOLD_STAR, BOLD_UNDERSCORE, ITALIC_STAR, ITALIC_UNDERSCORE, STRIKETHROUGH, INLINE_CODE]} />
        <CodeExclusivityPlugin />
        </div>
      </div>
    </LexicalComposer>
  )
}
