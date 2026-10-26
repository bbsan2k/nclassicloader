// Host-side visible-window check, separate from the app's offscreen stage probe.
import Foundation
import CoreGraphics
import ImageIO

enum ScreenshotError: Error { case invalidArguments, decode, bitmap, blank }

func checkScreenshot() throws {
    guard CommandLine.arguments.count == 3 else { throw ScreenshotError.invalidArguments }
    let url = URL(fileURLWithPath: CommandLine.arguments[1])
    guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { throw ScreenshotError.decode }
    let width = image.width, height = image.height
    guard width > 0, height > 0 else { throw ScreenshotError.decode }
    var bytes = [UInt8](repeating: 0, count: width * height * 4)
    try bytes.withUnsafeMutableBytes { storage in
        guard let context = CGContext(data: storage.baseAddress, width: width, height: height,
            bitsPerComponent: 8, bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue)
        else { throw ScreenshotError.bitmap }
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
    }
    var nonblack = 0, colored = 0, contrasting = 0
    let background = bytes.prefix(3).map(Int.init)
    for offset in stride(from: 0, to: bytes.count, by: 4) {
        let r = Int(bytes[offset]), g = Int(bytes[offset + 1]), b = Int(bytes[offset + 2])
        let maximum = max(r, g, b), minimum = min(r, g, b)
        if maximum > 16 { nonblack += 1 }
        if maximum > 32 && maximum - minimum > 16 { colored += 1 }
        if abs(r-background[0]) + abs(g-background[1]) + abs(b-background[2]) > 90 { contrasting += 1 }
    }
    let requiresColor = ["preview", "game", "input"].contains(CommandLine.arguments[2])
    let nativeUI = CommandLine.arguments[2] == "native"
    let passed = nonblack > 100 && (!requiresColor || colored > width * height / 20)
        && (!nativeUI || contrasting > width * height / 200)
    let report: [String: Any] = ["width": width, "height": height, "nonblack": nonblack,
                                "colored": colored, "contrasting": contrasting, "passed": passed]
    let json = try JSONSerialization.data(withJSONObject: report, options: [.sortedKeys])
    print(String(decoding: json, as: UTF8.self))
    if !passed { throw ScreenshotError.blank }
}

do { try checkScreenshot() }
catch { fputs("Screenshot check failed: \(error)\n", stderr); exit(1) }
