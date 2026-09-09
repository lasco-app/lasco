import AVKit
import Observation
import SwiftUI
import UniformTypeIdentifiers

@MainActor
@Observable
final class MediaGalleryAssetCache {
    private(set) var fullImages: [FfiMediaUuid: Image] = [:]
    private(set) var thumbnails: [FfiMediaUuid: Image] = [:]
    private(set) var videoPlayers: [FfiMediaUuid: AVPlayer] = [:]
    private(set) var livePhotoVideoItems: [FfiMediaUuid: FfiMediaItem] = [:]

    private let repository: any LibraryRepositoryProtocol
    private var positionByMediaID: [FfiMediaUuid: Int] = [:]
    private var imageLoads = Set<FfiMediaUuid>()
    private var playerLoads = Set<FfiMediaUuid>()
    private var livePhotoLoads = Set<FfiMediaUuid>()
    private var selectedPosition: Int

    init(repository: any LibraryRepositoryProtocol, initialPosition: Int) {
        self.repository = repository
        selectedPosition = initialPosition
    }

    func select(_ position: Int) {
        selectedPosition = position
        evict(around: position)
    }

    func isVideo(_ item: FfiMediaItem) -> Bool {
        let ext = (item.filenameOriginal as NSString).pathExtension.lowercased()
        return UTType(filenameExtension: ext)?.conforms(to: .audiovisualContent) == true
    }

    func player(for item: FfiMediaItem) -> AVPlayer? {
        videoPlayers[item.mediaId]
    }

    func loadLivePhotoVideo(
        for item: FfiMediaItem,
        at position: Int
    ) async {
        guard let videoID = item.appleLivePhotoMediaId,
              livePhotoVideoItems[item.mediaId] == nil,
              livePhotoLoads.insert(item.mediaId).inserted else { return }
        defer { livePhotoLoads.remove(item.mediaId) }
        do {
            let video = try await repository.showMedia(id: videoID)
            guard !Task.isCancelled,
                  isRetained(position, radius: 2) else { return }
            livePhotoVideoItems[item.mediaId] = video
            positionByMediaID[item.mediaId] = position
            positionByMediaID[video.mediaId] = position
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "Live Photo video query failed: \(error)")
        }
    }

    func loadPlayer(
        for item: FfiMediaItem,
        at position: Int
    ) async {
        guard videoPlayers[item.mediaId] == nil,
              playerLoads.insert(item.mediaId).inserted else { return }
        defer { playerLoads.remove(item.mediaId) }
        do {
            let url = try await repository.materializedMediaURL(
                mediaID: item.mediaId,
                originalFilename: item.filenameOriginal
            )
            guard !Task.isCancelled,
                  isRetained(position, radius: 1) else { return }
            if videoPlayers[item.mediaId] == nil {
                videoPlayers[item.mediaId] = AVPlayer(url: url)
                positionByMediaID[item.mediaId] = position
            }
        } catch is CancellationError {
        } catch {
            AppLogger.log(.error, "video materialization failed: \(error)")
        }
    }

    func loadImages(
        for item: FfiMediaItem,
        at position: Int
    ) async {
        guard !isVideo(item),
              imageLoads.insert(item.mediaId).inserted else { return }
        defer { imageLoads.remove(item.mediaId) }
        if thumbnails[item.mediaId] == nil {
            do {
                let data = try await repository.thumbnailAsync(mediaID: item.mediaId)
                guard let image = Image(data: data),
                      !Task.isCancelled,
                      isRetained(position, radius: 4) else { return }
                thumbnails[item.mediaId] = image
                positionByMediaID[item.mediaId] = position
            } catch is CancellationError {
                return
            } catch {
                AppLogger.log(.error, "thumbnail load failed: \(error)")
            }
        }

        if fullImages[item.mediaId] == nil {
            do {
                let data = try await repository.nativeMediaBytesAsync(mediaID: item.mediaId)
                guard let image = Image(data: data),
                      !Task.isCancelled,
                      isRetained(position, radius: 2) else { return }
                fullImages[item.mediaId] = image
                positionByMediaID[item.mediaId] = position
            } catch is CancellationError {
            } catch {
                AppLogger.log(.error, "full image load failed: \(error)")
            }
        }
    }

    func evict(around position: Int) {
        let fullImageIDs = retainedIDs(around: position, radius: 2)
        let thumbnailIDs = retainedIDs(around: position, radius: 4)
        let playerIDs = retainedIDs(around: position, radius: 1)
        let livePhotoIDs = retainedIDs(around: position, radius: 2)

        fullImages = fullImages.filter { fullImageIDs.contains($0.key) }
        thumbnails = thumbnails.filter { thumbnailIDs.contains($0.key) }
        for mediaID in Array(videoPlayers.keys) where !playerIDs.contains(mediaID) {
            videoPlayers[mediaID]?.pause()
            videoPlayers[mediaID]?.replaceCurrentItem(with: nil)
            videoPlayers.removeValue(forKey: mediaID)
        }
        livePhotoVideoItems = livePhotoVideoItems.filter { livePhotoIDs.contains($0.key) }

        let retained = Set(fullImages.keys)
            .union(thumbnails.keys)
            .union(videoPlayers.keys)
            .union(livePhotoVideoItems.keys)
        positionByMediaID = positionByMediaID.filter { retained.contains($0.key) }
    }

    func stopAllPlayers() {
        for player in videoPlayers.values { player.pause() }
    }

    private func retainedIDs(around position: Int, radius: Int) -> Set<FfiMediaUuid> {
        Set(positionByMediaID.compactMap { mediaID, cachedPosition in
            abs(cachedPosition - position) <= radius ? mediaID : nil
        })
    }

    private func isRetained(_ position: Int, radius: Int) -> Bool {
        abs(position - selectedPosition) <= radius
    }
}
