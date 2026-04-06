import { useState } from 'react'
import {
  NodeViewWrapper,
  NodeViewContent,
  type NodeViewProps,
} from '@tiptap/react'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '~/components/ui/select'
import { Button } from '~/components/ui/button'
import { CopySimpleIcon, CheckIcon } from '@phosphor-icons/react'
import { CODE_LANGUAGES } from '../formatting-options'

export function TiptapCodeBlock({ node, updateAttributes, editor }: NodeViewProps) {
  const [copied, setCopied] = useState(false)
  const language = (node.attrs.language as string) || 'plaintext'

  const handleCopy = () => {
    navigator.clipboard.writeText(node.textContent)
    setCopied(true)
    setTimeout(() => setCopied(false), 1500)
  }

  const handleLanguageChange = (v: string) => {
    updateAttributes({ language: v })
    editor.commands.loadCodeLanguage(v as Parameters<typeof editor.commands.loadCodeLanguage>[0])
  }

  return (
    <NodeViewWrapper className="tiptap-code-block my-3 overflow-hidden rounded-lg border border-border">
      <div
        className="flex items-center justify-between border-b border-border/60 bg-black/10 px-3 py-1"
        contentEditable={false}
      >
        <Select
          value={language}
          onValueChange={handleLanguageChange}
        >
          <SelectTrigger className="h-6 w-32 border-0 bg-transparent px-1 text-xs text-muted-foreground shadow-none focus:ring-0 hover:text-foreground transition-colors">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {CODE_LANGUAGES.map((l) => (
              <SelectItem key={l.value} value={l.value} className="text-xs">
                {l.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          variant="ghost"
          size="sm"
          className="h-6 w-6 p-0 text-muted-foreground hover:text-foreground transition-colors"
          onClick={handleCopy}
          aria-label="Copy code"
        >
          {copied ? (
            <CheckIcon className="size-3.5 text-green-500" />
          ) : (
            <CopySimpleIcon className="size-3.5" />
          )}
        </Button>
      </div>
      <pre className="m-0 overflow-x-auto px-4 py-3 text-sm">
        <NodeViewContent<'code'> as="code" spellCheck={false} />
      </pre>
    </NodeViewWrapper>
  )
}
