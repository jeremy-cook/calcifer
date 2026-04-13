# Calcifer — Development Guidelines

## React Component Style

### Named interfaces for props
Always define a named interface for component props. Never use inline object types in function signatures.

```tsx
// Bad
function Foo({ label }: { label: string }) {}

// Bad
function Foo({ label, ...props }: React.ComponentProps<'div'> & { label: string }) {}

// Good
interface FooProps extends React.ComponentProps<'div'> {
  label: string
}
function Foo({ label, ...props }: FooProps) {}
```

Name the interface after the component: `ComponentNameProps`.

### Constants above the return, inside the function
Constants that are local to a component belong just above the `return`, inside the function body — not at module level (unless genuinely shared across multiple components).

```tsx
// Good
function Menu() {
  const items = [
    { label: 'Cut', icon: ScissorsIcon },
    { label: 'Copy', icon: CopyIcon },
  ] as const

  return (...)
}
```

### Named render functions for multi-line conditionals
When both branches of a conditional are more than ~1 line, extract each to a named render function above the return.

```tsx
// Bad
{file ? (
  <div>
    <span>{file.name}</span>
    <button onClick={() => setFile(null)}>×</button>
  </div>
) : (
  <input type="url" value={url} onChange={...} />
)}

// Good
const renderFilePreview = () => (
  <div>
    <span>{file!.name}</span>
    <button onClick={() => setFile(null)}>×</button>
  </div>
)

const renderUrlInput = () => (
  <input type="url" value={url} onChange={...} />
)

return (
  {file ? renderFilePreview() : renderUrlInput()}
)
```

### Sub-components for distinct logical sections
When a JSX block is a self-contained logical unit with its own state or props, extract it as a proper named component — not an inline render function.

```tsx
// Good: TablePicker owns its own hover state
function TablePicker({ editor, close }: TablePickerProps) {
  const [hovered, setHovered] = useState({ rows: 0, cols: 0 })
  return (...)
}
```

### Early returns to eliminate nesting
Return early for guard conditions instead of wrapping the main body in a conditional.

```tsx
// Bad
function Sections({ sections }: SectionsProps) {
  return (
    <>
      {sections.length > 0 && (
        <div>...</div>
      )}
    </>
  )
}

// Good
function Sections({ sections }: SectionsProps) {
  if (sections.length === 0) return null
  return <div>...</div>
}
```
