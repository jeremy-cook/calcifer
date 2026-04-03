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
import { CODE_LANGUAGES } from '~/editors/shared/formatting-options'

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
    <NodeViewWrapper className="tiptap-code-block my-3 overflow-hidden rounded-md border border-border">
      <div
        className="flex items-center justify-between border-b border-border bg-muted/50 px-3 py-1.5"
        contentEditable={false}
      >
        <Select
          value={language}
          onValueChange={handleLanguageChange}
        >
          <SelectTrigger className="h-6 w-32 border-0 bg-transparent px-1 text-xs shadow-none focus:ring-0">
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
          className="h-6 px-2 text-xs"
          onClick={handleCopy}
          aria-label="Copy code"
        >
          {copied ? (
            <CheckIcon className="text-green-500" />
          ) : (
            <CopySimpleIcon />
          )}
        </Button>
      </div>
      <pre className="m-0 overflow-x-auto p-4 text-sm">
        <NodeViewContent<'code'> as="code" />
      </pre>
    </NodeViewWrapper>
  )
}
