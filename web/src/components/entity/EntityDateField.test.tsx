// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { EntityDateField } from '~/components/entity/EntityDateField'

afterEach(cleanup)

describe('EntityDateField', () => {
  it('pages the calendar to the next month and picks a day there (I-85)', () => {
    const onChange = vi.fn()
    render(<EntityDateField label="Due" iso="2026-10-15" onChange={onChange} />)

    fireEvent.click(screen.getByRole('button', { name: /October/ }))
    fireEvent.click(screen.getByRole('button', { name: /next month/i }))
    fireEvent.click(screen.getByRole('button', { name: /November 10th/ }))

    expect(onChange).toHaveBeenCalledWith('2026-11-10')
  })

  it('opens on the selected date again after paging away', () => {
    render(<EntityDateField label="Due" iso="2026-10-15" onChange={vi.fn()} />)

    fireEvent.click(screen.getByRole('button', { name: /October/ }))
    fireEvent.click(screen.getByRole('button', { name: /next month/i }))
    fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' })
    fireEvent.click(screen.getByRole('button', { name: /October/ }))

    expect(screen.getByText('October 2026')).toBeTruthy()
  })
})
