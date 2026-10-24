import { useState } from 'react'
import { Input } from '~/components/ui/input'
import { useQuickAddTodo } from '~/model/todos'

export function TodoQuickAdd() {
  const [value, setValue] = useState('')
  const quickAddTodo = useQuickAddTodo()

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== 'Enter') return
    const text = value.trim()
    if (!text) return
    void quickAddTodo(text)
    setValue('')
  }

  return (
    <Input
      value={value}
      onChange={(e) => setValue(e.target.value)}
      onKeyDown={handleKeyDown}
      placeholder="Add a to-do…"
      aria-label="Quick add to-do"
    />
  )
}
