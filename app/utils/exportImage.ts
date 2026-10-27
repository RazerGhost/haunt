import type { SvgColors } from '~~/shared/exportSvg'

/*
 * Saving an exported picture (shared/exportSvg.ts): as SVG, or rasterised to a PNG at twice its size. In the app the
 * save dialog picks the file and Rust writes it (`save_export`); in a browser, while Haunt is worked on, the browser
 * downloads it.
 */

export type ImageFormat = 'png' | 'svg'

/** The theme's colours as the canvas reads them, for the picture: the light or dark one now shown. */
export function themeColors(): SvgColors {
  const layers = new Map<string, string>()
  return {
    background: resolvedColor('--ui-bg'),
    elevated: resolvedColor('--ui-bg-elevated'),
    border: resolvedColor('--ui-border'),
    text: resolvedColor('--ui-text'),
    muted: resolvedColor('--ui-text-muted'),
    layer: (layer) => {
      let found = layers.get(layer)
      if (!found) {
        found = resolvedColor(layerVar(layer))
        layers.set(layer, found)
      }
      return found
    }
  }
}

/** An SVG drawn on a canvas at twice its size (less when that is more than a canvas holds), as a PNG. */
async function rasterise(svg: string): Promise<Blob> {
  const url = URL.createObjectURL(new Blob([svg], { type: 'image/svg+xml' }))
  try {
    const image = new Image()
    image.src = url
    await image.decode()
    const scale = Math.min(2, 16384 / image.naturalWidth, 16384 / image.naturalHeight)
    const canvas = document.createElement('canvas')
    canvas.width = Math.round(image.naturalWidth * scale)
    canvas.height = Math.round(image.naturalHeight * scale)
    const context = canvas.getContext('2d')
    if (!context) throw new Error(t('utils.exportImage.noCanvas'))
    context.drawImage(image, 0, 0, canvas.width, canvas.height)
    return await new Promise<Blob>((resolve, reject) => canvas.toBlob(blob => (blob ? resolve(blob) : reject(new Error(t('utils.exportImage.tooBig')))), 'image/png'))
  } finally {
    URL.revokeObjectURL(url)
  }
}

/** A blob as base64, without the data URL's prefix. */
function base64Of(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(String(reader.result).replace(/^data:[^,]*,/, ''))
    reader.onerror = () => reject(reader.error)
    reader.readAsDataURL(blob)
  })
}

/**
 * Saves `svg` as `format` under a name the save dialog starts with (`title` above it, `filter` naming the kind of
 * file). The path saved to (in a browser: the file's name); null when the dialog was closed.
 */
export async function saveImage(svg: string, format: ImageFormat, name: string, title: string, filter: string): Promise<string | null> {
  const inApp = import.meta.client && '__TAURI__' in window
  let path = name
  if (inApp) {
    const { save } = await import('@tauri-apps/plugin-dialog')
    const chosen = await save({ title, defaultPath: name, filters: [{ name: filter, extensions: [format] }] })
    if (!chosen) return null
    path = chosen
  }
  const blob = format === 'png' ? await rasterise(svg) : new Blob([svg], { type: 'image/svg+xml' })
  if (inApp) {
    await haunt.saveExport(path, format === 'png' ? await base64Of(blob) : svg, format === 'png')
    return path
  }
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = name
  link.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
  return name
}
