import Foundation
import Compression

enum AAEDecoder {

    static func decodeAdjustmentJSON(from aaeFileData: Data) -> String? {
        guard let plist = try? PropertyListSerialization.propertyList(from: aaeFileData, options: [], format: nil) as? [String: Any] else {
            AppLogger.log(.error, "AAE decode failed, could not parse plist")
            return nil
        }
        guard let adjustmentData = plist["adjustmentData"] as? Data else {
            AppLogger.log(.error, "AAE decode failed, no adjustmentData key in plist")
            return nil
        }
        guard let inflated = inflateRawDeflate(adjustmentData) else {
            AppLogger.log(.error, "AAE decode failed, could not inflate adjustmentData")
            return nil
        }
        guard let jsonObject = try? JSONSerialization.jsonObject(with: inflated),
              let pretty = try? JSONSerialization.data(withJSONObject: jsonObject, options: [.prettyPrinted, .sortedKeys]),
              let text = String(data: pretty, encoding: .utf8) else {
            AppLogger.log(.error, "AAE decode failed, inflated adjustmentData is not JSON")
            return nil
        }
        return text
    }

    private static func inflateRawDeflate(_ data: Data) -> Data? {
        var stream = compression_stream(
            dst_ptr: UnsafeMutablePointer<UInt8>(bitPattern: 1)!,
            dst_size: 0,
            src_ptr: UnsafeMutablePointer<UInt8>(bitPattern: 1)!,
            src_size: 0,
            state: nil
        )
        var status = compression_stream_init(&stream, COMPRESSION_STREAM_DECODE, COMPRESSION_ZLIB)
        guard status == COMPRESSION_STATUS_OK else { return nil }
        defer { compression_stream_destroy(&stream) }

        let bufferSize = 64 * 1024
        var destBuffer = [UInt8](repeating: 0, count: bufferSize)
        var output = Data()

        return data.withUnsafeBytes { (srcRaw: UnsafeRawBufferPointer) -> Data? in
            guard let srcBase = srcRaw.bindMemory(to: UInt8.self).baseAddress else { return nil }
            stream.src_ptr = srcBase
            stream.src_size = data.count

            repeat {
                let bytesWritten = destBuffer.withUnsafeMutableBufferPointer { destPtr -> Int in
                    stream.dst_ptr = destPtr.baseAddress!
                    stream.dst_size = bufferSize
                    status = compression_stream_process(&stream, Int32(COMPRESSION_STREAM_FINALIZE.rawValue))
                    return bufferSize - stream.dst_size
                }
                output.append(destBuffer, count: bytesWritten)
            } while status == COMPRESSION_STATUS_OK

            return status == COMPRESSION_STATUS_END ? output : nil
        }
    }
}
