import Foundation

actor LibraryChangeHub {
    private var continuations: [UUID: AsyncStream<LibraryChange>.Continuation] = [:]
    private var isFinished = false

    func changes() -> AsyncStream<LibraryChange> {
        let id = UUID()
        let (stream, continuation) = AsyncStream.makeStream(of: LibraryChange.self)

        if isFinished {
            continuation.finish()
            return stream
        }

        continuation.onTermination = { [hub = self] _ in
            Task { await hub.removeSubscriber(id) }
        }
        continuations[id] = continuation
        return stream
    }

    func notify(_ change: LibraryChange) {
        guard !isFinished else { return }
        for continuation in continuations.values {
            continuation.yield(change)
        }
    }

    func finish() {
        guard !isFinished else { return }
        isFinished = true
        for continuation in continuations.values {
            continuation.finish()
        }
        continuations.removeAll()
    }

    private func removeSubscriber(_ id: UUID) {
        continuations.removeValue(forKey: id)
    }
}

