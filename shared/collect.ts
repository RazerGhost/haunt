/** Adds `value` to the list kept under `key`, starting one when there is none: in place, not a copy per item. */
export function addTo<K, V>(map: Map<K, V[]>, key: K, ...values: V[]): void {
  const list = map.get(key)
  if (list) list.push(...values)
  else map.set(key, [...values])
}
