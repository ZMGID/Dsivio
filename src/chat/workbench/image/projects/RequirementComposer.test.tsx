import { fireEvent, render, screen } from '@testing-library/react'
import { expect, it, vi } from 'vitest'
import { RequirementComposer } from './RequirementComposer'
it('edits requirements without assistant or optimization controls', () => {
 const change = vi.fn()
 render(<RequirementComposer label="要求" value="" placeholder="描述" onChange={change} />)
 fireEvent.change(screen.getByRole('textbox'), { target: { value: '替换 logo' } })
 expect(change).toHaveBeenCalledWith('替换 logo')
 expect(screen.queryByRole('button')).toBeNull()
})
