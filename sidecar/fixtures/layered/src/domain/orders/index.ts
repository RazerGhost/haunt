import { save } from '../../infra/db'
import { money } from '../shared'
import type { Theme } from '../../app/ui-types'
import { format } from '../billing/format'

export const theme: Theme = 'light'

export function placeOrder(): number {
  format()
  return save(money(1))
}
