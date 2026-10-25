import { placeOrder } from '../domain/orders'
import { render } from './ui'

export function main(): string {
  return render(placeOrder())
}
