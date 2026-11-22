// Renders the Tauri icon source (1024x1024 PNG) from the Swift app's Icon Composer layer.
// Usage: swift render-icon.swift <Image Layer.png> <output.png>
import AppKit

let args = CommandLine.arguments
guard args.count == 3, let layer = NSImage(contentsOfFile: args[1]) else {
    FileHandle.standardError.write(Data("usage: render-icon.swift <layer.png> <out.png>\n".utf8))
    exit(1)
}

let size = 1024
guard let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
    samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
    bytesPerRow: 0, bitsPerPixel: 0
) else { exit(1) }
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
// macOS icon grid: an 824-point body centred on the 1024 canvas.
let body = NSRect(x: 100, y: 100, width: 824, height: 824)
let shape = NSBezierPath(roundedRect: body, xRadius: 185, yRadius: 185)
NSGradient(
    starting: NSColor(white: 1.0, alpha: 1), ending: NSColor(white: 0.93, alpha: 1)
)?.draw(in: shape, angle: -90)
// The cassette layer, about two thirds of the canvas wide, slightly above centre.
let width = 672.0
let height = width * layer.size.height / layer.size.width
let rect = NSRect(x: (1024 - width) / 2, y: 1024 * 0.54 - height / 2, width: width, height: height)
layer.draw(in: rect, from: .zero, operation: .sourceOver, fraction: 1)
NSGraphicsContext.restoreGraphicsState()
guard let png = rep.representation(using: .png, properties: [:]) else { exit(1) }
try png.write(to: URL(fileURLWithPath: args[2]))
