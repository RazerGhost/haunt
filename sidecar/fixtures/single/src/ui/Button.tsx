export function Button(props: { label: string }) {
  const onClick = () => props.label
  return <button onClick={onClick}>{props.label}</button>
}
