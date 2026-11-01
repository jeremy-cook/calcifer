import { useState } from 'react'
import { Input } from '~/components/ui/input'
import { parseTodoInput, useQuickAddTodo } from '~/model/todos'

// The text stays until the to-do is saved, so a refused or failed add can be
// fixed and resent.
export function TodoQuickAdd() {
  const [value, setValue] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)
  const quickAddTodo = useQuickAddTodo()

  const submit = async (text: string) => {
    const parsed = parseTodoInput(text)
    if (!parsed.ok) {
      setError(parsed.error)
      return
    }
    setPending(true)
    try {
      await quickAddTodo(parsed)
      // Keep anything typed while the add was in flight.
      setValue((current) => (current === text ? '' : current))
    } catch (err) {
      console.error('quick add failed', err)
      setError("Couldn't add the to-do.")
    } finally {
      setPending(false)
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== 'Enter' || pending || !value.trim()) return
    void submit(value)
  }

  const handleChange = (next: string) => {
    setValue(next)
    setError(null)
  }

  return (
    <div className="flex flex-col gap-1">
      <Input
        value={value}
        onChange={(e) => handleChange(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Add a to-do…"
        aria-label="Quick add to-do"
        aria-invalid={error !== null}
        aria-describedby={error ? 'todo-quick-add-error' : undefined}
        className="h-9 rounded-md bg-muted px-3 dark:bg-muted"
      />
      {error && (
        <p id="todo-quick-add-error" className="text-xs text-destructive">
          {error}
        </p>
      )}
    </div>
  )
}
