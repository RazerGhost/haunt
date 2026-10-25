/* Small drawing helpers for the 2D canvas (CodeCanvas.vue), with nothing of the canvas's own state. */

interface Point { x: number, y: number }
interface Rect { x: number, y: number, w: number, h: number }

/** A color read as `rgb(…)` with the opacity `a`. */
export function alpha(rgb: string, a: number): string {
  return rgb.replace('rgb(', 'rgba(').replace(')', `, ${a})`)
}

/** Writes `text` at most `max` wide, cut short with … when it is wider; nothing when not even one letter fits. */
export function fitText(ctx: CanvasRenderingContext2D, text: string, x: number, y: number, max: number): void {
  if (ctx.measureText(text).width <= max) {
    ctx.fillText(text, x, y)
    return
  }
  let cut = text.length
  while (cut > 1 && ctx.measureText(`${text.slice(0, cut)}…`).width > max) cut--
  if (cut > 1) ctx.fillText(`${text.slice(0, cut)}…`, x, y)
}

/** A line from `start` to `end` with an arrowhead, curved slightly so lines between neighbours do not lie on top of each other. */
export function curvedArrow(ctx: CanvasRenderingContext2D, start: Point, end: Point, color: string, dashed: boolean): void {
  ctx.save()
  ctx.strokeStyle = color
  ctx.fillStyle = color
  ctx.lineWidth = 1.5
  if (dashed) ctx.setLineDash([5, 4])
  ctx.beginPath()
  ctx.moveTo(start.x, start.y)
  const mx = (start.x + end.x) / 2 - (end.y - start.y) * 0.15
  const my = (start.y + end.y) / 2 + (end.x - start.x) * 0.15
  ctx.quadraticCurveTo(mx, my, end.x, end.y)
  ctx.stroke()
  const angle = Math.atan2(end.y - my, end.x - mx)
  ctx.setLineDash([])
  ctx.beginPath()
  ctx.moveTo(end.x, end.y)
  ctx.lineTo(end.x - 8 * Math.cos(angle - 0.4), end.y - 8 * Math.sin(angle - 0.4))
  ctx.lineTo(end.x - 8 * Math.cos(angle + 0.4), end.y - 8 * Math.sin(angle + 0.4))
  ctx.fill()
  ctx.restore()
}

/** Lines of code inside a function's box `s`, under its name, as many as fit, clipped to the box. */
export function codeLines(ctx: CanvasRenderingContext2D, lines: string[], s: Rect, color: string): void {
  const fontSize = Math.max(9, Math.min(13, s.h / 24))
  const lineHeight = fontSize * 1.4
  ctx.save()
  ctx.beginPath()
  ctx.rect(s.x + 4, s.y + 22, s.w - 8, s.h - 26)
  ctx.clip()
  ctx.font = `${fontSize}px ui-monospace, monospace`
  ctx.fillStyle = color
  ctx.textBaseline = 'top'
  const shown = Math.floor((s.h - 28) / lineHeight)
  lines.slice(0, shown).forEach((line, at) => ctx.fillText(line.replace(/\t/g, '  '), s.x + 8, s.y + 24 + at * lineHeight))
  ctx.restore()
}
