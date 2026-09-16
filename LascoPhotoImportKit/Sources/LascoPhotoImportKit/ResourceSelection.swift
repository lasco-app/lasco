import Foundation

/// Platform-neutral resource selection. PhotoKit discovery supplies its available resource types;
/// this function is shared by every caller and is deliberately free of FFI concepts.
public enum ResourceSelection {
    public struct Candidate: Hashable, Sendable {
        public let ticket: String
        public let type: PhotosResourceType
        public let filename: String
        public let byteCount: Int64

        public init(ticket: String, type: PhotosResourceType, filename: String, byteCount: Int64) {
            self.ticket = ticket
            self.type = type
            self.filename = filename
            self.byteCount = byteCount
        }
    }

    public static func select(from candidates: [Candidate]) -> [(candidate: Candidate, role: PhotosResourceRole)] {
        func first(_ types: PhotosResourceType...) -> Candidate? {
            types.lazy.compactMap { type in candidates.first { $0.type == type } }.first
        }
        let photo = first(.photo)
        let fullSizePhoto = first(.fullSizePhoto)
        let adjustment = first(.adjustmentData)
        let paired = first(.pairedVideo, .fullSizePairedVideo)
        let video = first(.video, .fullSizeVideo)
        let editedStill = photo != nil && fullSizePhoto != nil
        var selected: [(Candidate, PhotosResourceRole)] = []
        if editedStill, let adjustment { selected.append((adjustment, .adjustmentData)) }
        if (photo != nil || fullSizePhoto != nil), let paired { selected.append((paired, .pairedVideo)) }
        if let photo { selected.append((photo, .primary)) }
        else if let fullSizePhoto { selected.append((fullSizePhoto, .primary)) }
        else if let paired { selected.append((paired, .primary)) }
        else if let video { selected.append((video, .primary)) }
        return selected
    }
}
