import { useState } from 'react'
import { Input } from '~/components/ui/input'
import { useCreateEntity } from '~/model/store'

export function TodoQuickAdd() {
  const [value, setValue] = useState('')
  const createEntity = useCreateEntity()

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== 'Enter') return
    const name = value.trim()
    if (!name) return
    void createEntity('Todo', name)
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
