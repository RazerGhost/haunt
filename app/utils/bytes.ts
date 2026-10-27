/** A size in bytes for people: 512 bytes, 3.4 KB, 12 MB, 1.2 GB. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return t('utils.bytes.bytes', { count: bytes })
  const units = ['KB', 'MB', 'GB', 'TB']
  let size = bytes / 1024
  let unit = 0
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024
    unit++
  }
  return `${size < 10 ? size.toFixed(1) : Math.round(size)} ${units[unit]}`
}
