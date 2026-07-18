import CoreGraphics
import AVFoundation
import ImageIO
import UniformTypeIdentifiers

/// Max pixel dimension for thumbnails — must match `THUMBNAIL_SIZE` in lasco-core.
let thumbnailSize: Int = 256

enum ThumbnailGenerator {
    /// Generates a JPEG thumbnail for the file at `url`.
    /// Returns `nil` if the file type is unsupported or generation fails.
    static func generate(for url: URL) -> Data? {
        let uti = UTType(filenameExtension: url.pathExtension.lowercased())
        if uti?.conforms(to: .movie) == true || uti?.conforms(to: .video) == true {
            return videoThumbnail(url: url)
        }
        return imageThumbnail(url: url)
    }

    private static func imageThumbnail(url: URL) -> Data? {
        guard let src = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
        let opts: [CFString: Any] = [
            kCGImageSourceThumbnailMaxPixelSize: thumbnailSize,
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
        ]
        guard let cgImage = CGImageSourceCreateThumbnailAtIndex(src, 0, opts as CFDictionary) else {
            return nil
        }
        return jpegData(from: cgImage)
    }

    private static func videoThumbnail(url: URL) -> Data? {
        let asset = AVURLAsset(url: url)
        let gen = AVAssetImageGenerator(asset: asset)
        gen.maximumSize = CGSize(width: thumbnailSize, height: thumbnailSize)
        gen.appliesPreferredTrackTransform = true
        var result: Data?
        let sema = DispatchSemaphore(value: 0)
        gen.generateCGImageAsynchronously(for: .zero) { cgImage, _, _ in
            if let cgImage { result = jpegData(from: cgImage) }
            sema.signal()
        }
        sema.wait()
        return result
    }

    private static func jpegData(from cgImage: CGImage) -> Data? {
        let data = NSMutableData()
        guard let dest = CGImageDestinationCreateWithData(data, UTType.jpeg.identifier as CFString, 1, nil) else {
            return nil
        }
        let opts: [CFString: Any] = [kCGImageDestinationLossyCompressionQuality: 0.8]
        CGImageDestinationAddImage(dest, cgImage, opts as CFDictionary)
        guard CGImageDestinationFinalize(dest) else { return nil }
        return data as Data
    }
}
