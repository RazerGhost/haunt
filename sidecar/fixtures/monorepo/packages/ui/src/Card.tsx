import { greet } from '@demo/core'

export const Card = (props: { name: string }) => <div>{greet(props.name)}</div>
