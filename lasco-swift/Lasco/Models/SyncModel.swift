import Foundation

extension LibraryModel {

    // MARK: - SyncRecord

    struct SyncRecord {
        let date: Date
        let success: Bool
    }

    // MARK: - Default fetch remote

    func setDefaultFetchRemote(remoteId: String?) {
        do { try lib?.setDefaultFetchRemote(remoteId: remoteId) }
        catch { AppLogger.log(.error, "setDefaultFetchRemote failed: \(error)") }
        defaultFetchRemoteId = remoteId
    }

    func fetchDefaultRemote() async {
        guard let remoteId = defaultFetchRemoteId, isFetchAllowed(remoteId) else { return }
        _ = await fetchRemote(remoteId: remoteId)
    }

    // MARK: - Busy state

    func isPushAllowed(_ remoteId: String) -> Bool {
        !busyRemotes.contains(remoteId)
    }

    func isFetchAllowed(_ remoteId: String) -> Bool {
        !busyRemotes.contains(remoteId) && !fetchInProgress
    }

    // MARK: - Sync record helpers

    func recordSync(key: String, remoteId: String, success: Bool, records: inout [String: SyncRecord]) {
        let now = Date()
        UserDefaults.standard.set(now, forKey: "\(key).\(remoteId)")
        UserDefaults.standard.set(success, forKey: "\(key)Ok.\(remoteId)")
        records[remoteId] = SyncRecord(date: now, success: success)
    }

    // MARK: - Sync

    func sync() async {
        AppLogger.log(.info, "sync started")
        let remoteId = defaultFetchRemoteId
        if let remoteId { busyRemotes.insert(remoteId) }
        fetchInProgress = true
        defer {
            if let remoteId { busyRemotes.remove(remoteId) }
            fetchInProgress = false
        }
        do {
            _ = try await lib?.syncAsync(appSupportDir: appSupportDirPath)
            AppLogger.log(.info, "sync succeeded")
        } catch {
            AppLogger.log(.error, "sync failed: \(error)")
        }
        reload()
    }

    func pushRemote(remoteId: String) async -> String? {
        AppLogger.log(.info, "pushRemote started — remoteId: \(remoteId)")
        busyRemotes.insert(remoteId)
        defer { busyRemotes.remove(remoteId) }
        do {
            _ = try await lib?.pushRemoteAsync(remoteId: remoteId, appSupportDir: appSupportDirPath)
            recordSync(key: "lasco.lastPush", remoteId: remoteId, success: true, records: &lastPushRecords)
            AppLogger.log(.info, "pushRemote succeeded — remoteId: \(remoteId)")
            error = nil
            return nil
        } catch let e as LascoError {
            AppLogger.log(.error, "pushRemote failed — remoteId: \(remoteId): \(e)")
            recordSync(key: "lasco.lastPush", remoteId: remoteId, success: false, records: &lastPushRecords)
            error = e.friendlyMessage
            return e.friendlyMessage
        } catch {
            AppLogger.log(.error, "pushRemote failed — remoteId: \(remoteId): \(error)")
            recordSync(key: "lasco.lastPush", remoteId: remoteId, success: false, records: &lastPushRecords)
            self.error = error.localizedDescription
            return error.localizedDescription
        }
    }

    func fetchRemote(remoteId: String) async -> String? {
        AppLogger.log(.info, "fetchRemote started — remoteId: \(remoteId)")
        busyRemotes.insert(remoteId)
        fetchInProgress = true
        defer {
            busyRemotes.remove(remoteId)
            fetchInProgress = false
        }
        do {
            _ = try await lib?.fetchRemoteAsync(remoteId: remoteId, appSupportDir: appSupportDirPath)
            recordSync(key: "lasco.lastFetch", remoteId: remoteId, success: true, records: &lastFetchRecords)
            reload()
            AppLogger.log(.info, "fetchRemote succeeded — remoteId: \(remoteId)")
            error = nil
            return nil
        } catch let e as LascoError {
            AppLogger.log(.error, "fetchRemote failed — remoteId: \(remoteId): \(e)")
            recordSync(key: "lasco.lastFetch", remoteId: remoteId, success: false, records: &lastFetchRecords)
            error = e.friendlyMessage
            return e.friendlyMessage
        } catch {
            AppLogger.log(.error, "fetchRemote failed — remoteId: \(remoteId): \(error)")
            recordSync(key: "lasco.lastFetch", remoteId: remoteId, success: false, records: &lastFetchRecords)
            self.error = error.localizedDescription
            return error.localizedDescription
        }
    }

    func schedulePush() {
        pushDebounceTask?.cancel()
        nextPushDate = Date().addingTimeInterval(30)
        pushDebounceTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(30))
            guard let self, !Task.isCancelled else { return }
            await MainActor.run { self.nextPushDate = nil }
            for remote in await self.remotes {
                _ = await self.pushRemote(remoteId: remote.id)
            }
        }
    }

    // MARK: - Remotes

    @discardableResult
    func addRemoteFixedPath(name: String, path: String) -> String? {
        AppLogger.log(.info, "adding fixed-path remote '\(name)' at \(path)")
        do {
            let remoteId = try lib?.addRemoteFixedPath(name: name, path: path)
            remotes = lib?.listRemotes() ?? []
            defaultFetchRemoteId = lib?.getDefaultFetchRemote()
            return remoteId
        } catch let e as LascoError {
            AppLogger.log(.error, "addRemoteFixedPath '\(name)' failed: \(e)")
            error = e.friendlyMessage
            return nil
        } catch {
            AppLogger.log(.error, "addRemoteFixedPath '\(name)' failed: \(error)")
            self.error = error.localizedDescription
            return nil
        }
    }

    func addRemoteDebugLocalApple(name: String) -> String? {
        AppLogger.log(.info, "adding debug-local-apple remote '\(name)'")
        do {
            let remoteId = try lib?.addRemoteDebugLocalApple(name: name)
            remotes = lib?.listRemotes() ?? []
            defaultFetchRemoteId = lib?.getDefaultFetchRemote()
            return remoteId
        } catch let e as LascoError {
            AppLogger.log(.error, "addRemoteDebugLocalApple '\(name)' failed: \(e)")
            error = e.friendlyMessage
            return nil
        } catch {
            AppLogger.log(.error, "addRemoteDebugLocalApple '\(name)' failed: \(error)")
            self.error = error.localizedDescription
            return nil
        }
    }

    @discardableResult
    func addRemoteS3(
        id: String,
        endpoint: String,
        bucket: String,
        region: String,
        accessKey: String,
        secretKey: String
    ) -> String? {
        AppLogger.log(.info, "adding s3 remote '\(id)' bucket \(bucket)")
        do {
            let remoteId = try lib?.addRemoteS3(
                name: id,
                endpoint: endpoint,
                bucket: bucket,
                region: region,
                accessKey: accessKey,
                secretKey: secretKey
            )
            remotes = lib?.listRemotes() ?? []
            defaultFetchRemoteId = lib?.getDefaultFetchRemote()
            return remoteId
        } catch let e as LascoError {
            AppLogger.log(.error, "addRemoteS3 '\(id)' failed: \(e)")
            error = e.friendlyMessage
            return nil
        } catch {
            AppLogger.log(.error, "addRemoteS3 '\(id)' failed: \(error)")
            self.error = error.localizedDescription
            return nil
        }
    }

    func removeRemote(id: String) {
        AppLogger.log(.info, "removing remote '\(id)'")
        let remote = remotes.first { $0.id == id }
        do {
            try lib?.removeRemote(remoteId: id)
            remotes = lib?.listRemotes() ?? []
            if let path = remote?.path, remote?.kind == "fixed_path" {
                try? FileManager.default.removeItem(atPath: path)
            }
            if let dirName = remote?.path, remote?.kind == "debug_local_apple",
               let root = appSupportDirPath {
                let path = "\(root)/lasco/local_fs_test/\(dirName)"
                try? FileManager.default.removeItem(atPath: path)
            }
        } catch let e as LascoError {
            AppLogger.log(.error, "removeRemote '\(id)' failed: \(e)")
            error = e.friendlyMessage
        } catch {
            AppLogger.log(.error, "removeRemote '\(id)' failed: \(error)")
            self.error = error.localizedDescription
        }
    }

    func initializeRemote(remoteId: String) async -> String? {
        AppLogger.log(.info, "initializeRemote started — remoteId: \(remoteId)")
        do {
            try lib?.initializeRemote(remoteId: remoteId, appSupportDir: appSupportDirPath)
            AppLogger.log(.info, "initializeRemote succeeded — remoteId: \(remoteId)")
            error = nil
            return nil
        } catch let e as LascoError {
            AppLogger.log(.error, "initializeRemote failed — remoteId: \(remoteId): \(e)")
            error = e.friendlyMessage
            return e.friendlyMessage
        } catch {
            AppLogger.log(.error, "initializeRemote failed — remoteId: \(remoteId): \(error)")
            self.error = error.localizedDescription
            return error.localizedDescription
        }
    }

    func connectRemote(remoteId: String) async -> Bool {
        AppLogger.log(.info, "connectRemote started — remoteId: \(remoteId)")
        do {
            try lib?.connectRemote(remoteId: remoteId, appSupportDir: appSupportDirPath)
            AppLogger.log(.info, "connectRemote succeeded — remoteId: \(remoteId)")
            error = nil
            return true
        } catch let e as LascoError {
            AppLogger.log(.error, "connectRemote failed — remoteId: \(remoteId): \(e)")
            error = e.friendlyMessage
            return false
        } catch {
            AppLogger.log(.error, "connectRemote failed — remoteId: \(remoteId): \(error)")
            self.error = error.localizedDescription
            return false
        }
    }
}
