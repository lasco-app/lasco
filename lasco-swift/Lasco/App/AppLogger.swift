import Foundation
import OSLog

enum AppLogger {
    private static let subsystem = "com.lasco.app"
    private static let maxFileSize: UInt64 = 2 * 1024 * 1024
    private static let maxBackups = 3

    static let logFileURL: URL = {
        let appSupport = FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return appSupport.appendingPathComponent("lasco/lasco.log")
    }()

    static var logFileURL_ifExists: URL? {
        FileManager.default.fileExists(atPath: logFileURL.path) ? logFileURL : nil
    }

    private static let osLog = OSLog(subsystem: subsystem, category: "app")
    private static let queue = DispatchQueue(label: "lasco.logger", qos: .utility)
    private static let dateFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd HH:mm:ss.SSS"
        return f
    }()

    static func setup() {
        let dir = logFileURL.deletingLastPathComponent()
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        if !FileManager.default.fileExists(atPath: logFileURL.path) {
            FileManager.default.createFile(atPath: logFileURL.path, contents: nil)
        }
        #if DEBUG
        print("📋 Log file: \(logFileURL.path)")
        #endif
        log(.info, "--- Lasco started ---")
    }

    static func log(_ level: Level, _ message: String, file: String = #file, function: String = #function) {
        let timestamp = dateFormatter.string(from: Date())
        let filename = URL(fileURLWithPath: file).lastPathComponent
        let line = "[\(level.label)] \(timestamp) \(filename):\(function) : \(message)\n"

        os_log("%{public}@", log: osLog, type: level.osLogType, message)

        queue.async {
            rotateIfNeeded()
            guard let data = line.data(using: .utf8),
                  let handle = try? FileHandle(forWritingTo: logFileURL) else { return }
            defer { handle.closeFile() }
            handle.seekToEndOfFile()
            handle.write(data)
        }
    }

    private static func rotateIfNeeded() {
        guard let attrs = try? FileManager.default.attributesOfItem(atPath: logFileURL.path),
              let size = attrs[.size] as? UInt64, size > maxFileSize else { return }

        let dir = logFileURL.deletingLastPathComponent()
        let base = logFileURL.deletingPathExtension().lastPathComponent
        let ext = logFileURL.pathExtension

        for i in stride(from: maxBackups - 1, through: 1, by: -1) {
            let src = dir.appendingPathComponent("\(base).\(i).\(ext)")
            let dst = dir.appendingPathComponent("\(base).\(i + 1).\(ext)")
            try? FileManager.default.removeItem(at: dst)
            try? FileManager.default.moveItem(at: src, to: dst)
        }
        let backup1 = dir.appendingPathComponent("\(base).1.\(ext)")
        try? FileManager.default.moveItem(at: logFileURL, to: backup1)
        FileManager.default.createFile(atPath: logFileURL.path, contents: nil)
    }

    enum Level {
        case debug, info, error

        var label: String {
            switch self {
            case .debug: return "DEBUG"
            case .info:  return "INFO "
            case .error: return "ERROR"
            }
        }

        var osLogType: OSLogType {
            switch self {
            case .debug: return .debug
            case .info:  return .info
            case .error: return .error
            }
        }
    }
}
