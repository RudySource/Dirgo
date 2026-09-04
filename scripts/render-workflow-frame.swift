#!/usr/bin/env swift

import AppKit
import Foundation

let arguments = CommandLine.arguments
guard arguments.count >= 7, let index = Int(arguments[1]) else {
    FileHandle.standardError.write(Data("usage: render-workflow-frame INDEX OUTPUT BADGE TITLE SUBTITLE [LINE ...]\n".utf8))
    exit(2)
}

let output = arguments[2]
let badge = arguments[3]
let title = arguments[4]
let subtitle = arguments[5]
let lines = Array(arguments.dropFirst(6))
let width = 1200
let height = 760

guard let bitmap = NSBitmapImageRep(
    bitmapDataPlanes: nil,
    pixelsWide: width,
    pixelsHigh: height,
    bitsPerSample: 8,
    samplesPerPixel: 4,
    hasAlpha: true,
    isPlanar: false,
    colorSpaceName: .deviceRGB,
    bitmapFormat: [],
    bytesPerRow: width * 4,
    bitsPerPixel: 32
), let context = NSGraphicsContext(bitmapImageRep: bitmap) else {
    fatalError("could not create bitmap context")
}

func color(_ hex: UInt32) -> NSColor {
    NSColor(
        red: CGFloat((hex >> 16) & 0xff) / 255,
        green: CGFloat((hex >> 8) & 0xff) / 255,
        blue: CGFloat(hex & 0xff) / 255,
        alpha: 1
    )
}

func rectFromTop(x: CGFloat, y: CGFloat, width: CGFloat, height: CGFloat) -> NSRect {
    NSRect(x: x, y: CGFloat(760) - y - height, width: width, height: height)
}

func drawText(_ value: String, x: CGFloat, top: CGFloat, font: NSFont, foreground: NSColor) {
    let attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: foreground]
    let measured = (value as NSString).size(withAttributes: attributes)
    (value as NSString).draw(
        at: NSPoint(x: x, y: CGFloat(760) - top - measured.height),
        withAttributes: attributes
    )
}

NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = context
color(0x07111F).setFill()
NSBezierPath(rect: NSRect(x: 0, y: 0, width: width, height: height)).fill()

drawText("DIRGO 0.8 · WORKFLOW INTELLIGENCE", x: 60, top: 42,
         font: .monospacedSystemFont(ofSize: 15, weight: .bold), foreground: color(0x30D158))
drawText(title, x: 60, top: 78, font: .systemFont(ofSize: 34, weight: .bold), foreground: color(0xF2F7FA))
drawText(subtitle, x: 60, top: 126, font: .systemFont(ofSize: 18), foreground: color(0x90A4B7))

let panel = NSBezierPath(roundedRect: rectFromTop(x: 52, y: 182, width: 1096, height: 430), xRadius: 24, yRadius: 24)
color(0x0D1B2A).setFill()
panel.fill()
color(0x20364D).setStroke()
panel.lineWidth = 1
panel.stroke()

for (x, fill) in [(82, 0xFF5F57), (102, 0xFEBC2E), (122, 0x28C840)] {
    color(UInt32(fill)).setFill()
    NSBezierPath(ovalIn: rectFromTop(x: CGFloat(x - 6), y: 206, width: 12, height: 12)).fill()
}

for (offset, line) in lines.enumerated() {
    drawText(line, x: 88, top: CGFloat(244 + offset * 48),
             font: .monospacedSystemFont(ofSize: 19, weight: .regular), foreground: color(0xD7E1EA))
}
drawText(badge, x: 88, top: 558, font: .monospacedSystemFont(ofSize: 14, weight: .bold), foreground: color(0x30D158))

color(0x20364D).setFill()
NSBezierPath(roundedRect: rectFromTop(x: 52, y: 640, width: 1096, height: 4), xRadius: 2, yRadius: 2).fill()
color(0x30D158).setFill()
NSBezierPath(roundedRect: rectFromTop(x: 52, y: 640, width: CGFloat(index * 137), height: 4), xRadius: 2, yRadius: 2).fill()
drawText("\(index) / 8", x: 1050, top: 660,
         font: .monospacedSystemFont(ofSize: 14, weight: .regular), foreground: color(0x74889B))

context.flushGraphics()
NSGraphicsContext.restoreGraphicsState()

guard let data = bitmap.representation(using: .png, properties: [:]) else {
    fatalError("could not encode PNG")
}
try data.write(to: URL(fileURLWithPath: output), options: .atomic)
