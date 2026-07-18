import Foundation

extension LibraryModel {

    // MARK: - Queries

    func showMedia(mediaId: String) -> FfiMediaItem? {
        try? lib?.showMedia(mediaId: mediaId)
    }

    func mediaInAlbum(albumId: String) -> [FfiMediaItem] {
        (try? lib?.mediaInAlbum(albumId: albumId)) ?? []
    }

    func containingAlbums(mediaId: String, excludingAlbumId: String? = nil) -> [FfiAlbum] {
        guard let ids = try? lib?.mediaContainingAlbumIds(mediaId: mediaId, includeViaGroups: true) else { return [] }
        return ids.compactMap { id in
            guard id != excludingAlbumId else { return nil }
            return albums.first { $0.albumId == id }
        }
    }

    func albumsContainingMedia(mediaId: String) -> [FfiAlbum] {
        guard let ids = try? lib?.mediaAlbumIds(mediaId: mediaId) else { return [] }
        return albums.filter { ids.contains($0.albumId) }
    }

    // MARK: - Thumbnail / bytes

    func thumbnail(for mediaId: String) -> Data? {
        do {
            let data = try lib?.getMediaThumbnail(mediaId: mediaId, appSupportDir: appSupportDirPath)
            AppLogger.log(.debug, "thumbnail \(mediaId) decrypted — \(data?.count ?? 0) bytes")
            return data
        } catch {
            AppLogger.log(.error, "getMediaThumbnail \(mediaId) failed: \(error)")
            return nil
        }
    }

    func mediaBytes(for mediaId: String) -> Data? {
        do {
            let data = try lib?.getMediaBytes(mediaId: mediaId, appSupportDir: appSupportDirPath)
            AppLogger.log(.debug, "media \(mediaId) decrypted — \(data?.count ?? 0) bytes")
            return data
        } catch {
            AppLogger.log(.error, "getMediaBytes \(mediaId) failed: \(error)")
            return nil
        }
    }

    func mediaBytesAsync(for mediaId: String) async -> Data? {
        do {
            let data = try await lib?.getMediaBytesAsync(mediaId: mediaId, appSupportDir: appSupportDirPath)
            AppLogger.log(.debug, "media \(mediaId) decrypted — \(data?.count ?? 0) bytes")
            return data
        } catch {
            AppLogger.log(.error, "mediaBytesAsync \(mediaId) failed: \(error)")
            return nil
        }
    }

    func thumbnailAsync(for mediaId: String) async -> Data? {
        do {
            let data = try await lib?.getMediaThumbnailAsync(mediaId: mediaId, appSupportDir: appSupportDirPath)
            AppLogger.log(.debug, "thumbnail \(mediaId) decrypted — \(data?.count ?? 0) bytes")
            return data
        } catch {
            AppLogger.log(.error, "thumbnailAsync \(mediaId) failed: \(error)")
            return nil
        }
    }

    func aaeAdjustmentJSON(for aaeMediaId: String) -> String? {
        guard let data = mediaBytes(for: aaeMediaId) else { return nil }
        return AAEDecoder.decodeAdjustmentJSON(from: data)
    }

    func videoURL(for mediaId: String, extension ext: String) -> URL? {
        if let cached = videoURLCache[mediaId] { return cached }
        guard let data = mediaBytes(for: mediaId) else { return nil }
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(mediaId)
            .appendingPathExtension(ext)
        do {
            try data.write(to: url)
            videoURLCache[mediaId] = url
            return url
        } catch {
            AppLogger.log(.error, "videoURL write to tmp for \(mediaId) failed: \(error)")
            return nil
        }
    }

    // MARK: - Mutations

    func renameMedia(mediaId: String, name: String?) {
        do { try lib?.renameMedia(mediaId: mediaId, name: name) }
        catch { AppLogger.log(.error, "renameMedia \(mediaId) failed: \(error)") }
        reload()
        schedulePush()
    }

    func deleteMedia(mediaId: String) {
        do { try lib?.deleteMedia(mediaId: mediaId) }
        catch { AppLogger.log(.error, "deleteMedia \(mediaId) failed: \(error)") }
        reload()
        schedulePush()
    }

    func addMediaToAlbum(albumId: String, mediaId: String) {
        do { try lib?.addMediaToAlbum(albumId: albumId, mediaId: mediaId) }
        catch { AppLogger.log(.error, "addMediaToAlbum \(mediaId) to \(albumId) failed: \(error)") }
        reload()
        schedulePush()
    }

    func removeMediaFromAlbum(albumId: String, mediaId: String) {
        do { try lib?.removeMediaFromAlbum(albumId: albumId, mediaId: mediaId) }
        catch { AppLogger.log(.error, "removeMediaFromAlbum \(mediaId) from \(albumId) failed: \(error)") }
        reload()
        schedulePush()
    }

    func moveMediaToAlbum(mediaId: String, fromAlbumId: String, toAlbumId: String) {
        do { try lib?.moveMediaToAlbum(mediaId: mediaId, fromAlbumId: fromAlbumId, toAlbumId: toAlbumId) }
        catch { AppLogger.log(.error, "moveMediaToAlbum \(mediaId) \(fromAlbumId)→\(toAlbumId) failed: \(error)") }
        reload()
        schedulePush()
    }

    // MARK: - Import

    func importMedia(urls: [URL], albumId: String) -> String? {
        AppLogger.log(.info, "importing \(urls.count) file(s) into album \(albumId)")
        var errors: [String] = []
        for url in urls {
            let accessed = url.startAccessingSecurityScopedResource()
            defer { if accessed { url.stopAccessingSecurityScopedResource() } }
            do {
                let mediaId = try lib?.importMedia(path: url.path, albumId: albumId, originalFilename: nil, appleAaeMediaId: nil, appleLivePhotoMediaId: nil)
                if let mediaId, let thumbData = ThumbnailGenerator.generate(for: url) {
                    try lib?.setMediaThumbnail(mediaId: mediaId, data: thumbData)
                }
            } catch let e as LascoError {
                AppLogger.log(.error, "import \(url.lastPathComponent) failed: \(e)")
                errors.append(e.friendlyMessage)
            } catch {
                AppLogger.log(.error, "import \(url.lastPathComponent) failed: \(error)")
                errors.append(error.localizedDescription)
            }
        }
        reload()
        AppLogger.log(.info, "import complete — \(urls.count - errors.count) ok, \(errors.count) failed")
        return errors.isEmpty ? nil : errors.first
    }

    func importMediaAsync(urls: [URL], albumId: String) async -> String? {
        AppLogger.log(.info, "importing \(urls.count) file(s) into album \(albumId)")
        isImporting = true
        let lib = self.lib
        let errors: [String] = await withCheckedContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                var errs: [String] = []
                for url in urls {
                    let accessed = url.startAccessingSecurityScopedResource()
                    defer { if accessed { url.stopAccessingSecurityScopedResource() } }
                    do {
                        let mediaId = try lib?.importMedia(path: url.path, albumId: albumId, originalFilename: nil, appleAaeMediaId: nil, appleLivePhotoMediaId: nil)
                        if let mediaId, let thumbData = ThumbnailGenerator.generate(for: url) {
                            try lib?.setMediaThumbnail(mediaId: mediaId, data: thumbData)
                        }
                    } catch let e as LascoError {
                        AppLogger.log(.error, "import \(url.lastPathComponent) failed: \(e)")
                        errs.append(e.friendlyMessage)
                    } catch {
                        AppLogger.log(.error, "import \(url.lastPathComponent) failed: \(error)")
                        errs.append(error.localizedDescription)
                    }
                }
                continuation.resume(returning: errs)
            }
        }
        reload()
        isImporting = false
        AppLogger.log(.info, "import complete — \(urls.count - errors.count) ok, \(errors.count) failed")
        if let first = errors.first { error = first }
        if errors.isEmpty { schedulePush() }
        return errors.first
    }
}
