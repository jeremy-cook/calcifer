import { TextAlignLeftIcon, TextAlignCenterIcon, TextAlignRightIcon, TextAlignJustifyIcon } from '@phosphor-icons/react'

export type Alignment = 'left' | 'center' | 'right' | 'justify'

export const ALIGNMENTS: { value: Alignment; icon: React.ReactNode; label: string }[] = [
  { value: 'left', icon: <TextAlignLeftIcon />, label: 'Align left' },
  { value: 'center', icon: <TextAlignCenterIcon />, label: 'Align center' },
  { value: 'right', icon: <TextAlignRightIcon />, label: 'Align right' },
  { value: 'justify', icon: <TextAlignJustifyIcon />, label: 'Justify' },
]

export const BLOCK_TYPES = [
  { label: 'Paragraph', value: 'paragraph' },
  { label: 'Heading 1', value: 'h1' },
  { label: 'Heading 2', value: 'h2' },
  { label: 'Heading 3', value: 'h3' },
  { label: 'Quote', value: 'blockquote' },
  { label: 'Code Block', value: 'code' },
]

export const CODE_LANGUAGES = [
  { label: 'Plain Text', value: 'plaintext' },
  { label: 'JavaScript', value: 'javascript' },
  { label: 'TypeScript', value: 'typescript' },
  { label: 'HTML', value: 'html' },
  { label: 'CSS', value: 'css' },
  { label: 'JSON', value: 'json' },
  { label: 'Python', value: 'python' },
  { label: 'Rust', value: 'rust' },
  { label: 'Go', value: 'go' },
  { label: 'SQL', value: 'sql' },
  { label: 'Bash', value: 'bash' },
  { label: 'Markdown', value: 'markdown' },
]

// Text colours drawn from the hearth palette, plus a cool pair for contrast.
export const TEXT_SWATCHES = ['#f2b630', '#f08a24', '#d9581f', '#ff7a45', '#a9a65a', '#6fa3a0', '#8c7bd0', '#9a8270']

// The same hues, translucent, so highlighted text stays readable.
export const HIGHLIGHT_SWATCHES = TEXT_SWATCHES.map((hex) => `${hex}55`)
