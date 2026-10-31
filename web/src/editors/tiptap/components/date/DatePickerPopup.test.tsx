// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { DatePickerPopup } from '~/editors/tiptap/components/date/DatePickerPopup'

afterEach(cleanup)

describe('DatePickerPopup', () => {
  it('picks a day in a month and year chosen from the selects (SR-04)', () => {
    const onSelect = vi.fn()
    render(<DatePickerPopup initialDate="2026-10-31" onSelect={onSelect} onClose={vi.fn()} />)

    fireEvent.change(screen.getByRole('combobox', { name: /year/i }), { target: { value: '1957' } })
    fireEvent.change(screen.getByRole('combobox', { name: /month/i }), { target: { value: '9' } })
    fireEvent.click(screen.getByRole('button', { name: /October 4th, 1957/ }))
    fireEvent.click(screen.getByRole('button', { name: 'Insert' }))

    expect(onSelect).toHaveBeenCalledWith('1957-10-04')
  })

  it('offers years from 1900 to ten years ahead', () => {
    render(<DatePickerPopup initialDate="2026-10-31" onSelect={vi.fn()} onClose={vi.fn()} />)

    const years = Array.from(screen.getByRole('combobox', { name: /year/i }).querySelectorAll('option')).map((o) =>
      Number(o.value),
    )

    expect(years[0]).toBe(1900)
    expect(years.at(-1)).toBe(new Date().getFullYear() + 10)
  })

  it('opens on the initial date', () => {
    render(<DatePickerPopup initialDate="1969-07-20" onSelect={vi.fn()} onClose={vi.fn()} />)

    expect(screen.getByRole('button', { name: /July 20th, 1969/ })).toBeTruthy()
  })
})
