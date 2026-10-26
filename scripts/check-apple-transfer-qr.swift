// Validate the actual compositor QR without exporting its pairing credential.
import Foundation
import Vision
import ImageIO

enum QRFailure: Error { case arguments, image, missing }
func check() throws {
    guard CommandLine.arguments.count == 2 else { throw QRFailure.arguments }
    let url = URL(fileURLWithPath: CommandLine.arguments[1])
    guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { throw QRFailure.image }
    let request = VNDetectBarcodesRequest()
    request.symbologies = [.qr]
    try VNImageRequestHandler(cgImage: image).perform([request])
    let matching = (request.results ?? []).filter { result in
        guard let text = result.payloadStringValue, let parts = URLComponents(string: text),
              parts.scheme == "http", parts.host != nil, parts.port != nil,
              let code = parts.fragment, code.count == 6 else { return false }
        return code.allSatisfy { "0123456789".contains($0) }
    }
    guard !matching.isEmpty else { throw QRFailure.missing }
    print("{\"passed\":true,\"paired_upload_qr_detected\":true}")
}
do { try check() }
catch { fputs("Transfer QR check failed: \(error)\n", stderr); exit(1) }
