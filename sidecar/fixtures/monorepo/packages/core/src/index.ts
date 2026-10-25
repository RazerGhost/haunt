import { shout } from './internal/format'

export function greet(name: string): string {
  return shout(`hello ${name}`)
}
