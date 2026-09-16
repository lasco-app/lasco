import SwiftUI
import UniformTypeIdentifiers

struct AddUsbRemoteView: View {
    @Environment(LibraryRepository.self) private var repository
    @Environment(ToastManager.self) private var toastManager
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) private var theme

    @State private var name = ""
    @State private var bookmarkBase64: String?
    @State private var selectedDriveName: String?
    @State private var selectedFolderPath: String?
    @State private var showingFolderPicker = false
    @State private var isAdding = false
    @State private var errorMessage: String?
    @State private var suggestedName: String?

    let onRemoteReady: @MainActor () async throws -> Void

    init(onRemoteReady: @escaping @MainActor () async throws -> Void = {}) {
        self.onRemoteReady = onRemoteReady
    }

    private var isValid: Bool {
        !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && bookmarkBase64 != nil
    }

    var body: some View {
        ZStack(alignment: .bottom) {
            theme.bg.ignoresSafeArea()

            VStack(spacing: 0) {
                HStack {
                    Spacer()
                    Button("Close", systemImage: "xmark", action: { dismiss() })
                        .labelStyle(.iconOnly)
                        .buttonStyle(.plain)
                        .foregroundStyle(theme.ink)
                        .accessibilityLabel("Close")
                }
                .padding(.horizontal, 32)
                .padding(.top, 32)
                .padding(.bottom, 16)

                Text("Add USB drive")
                    .font(LascoFont.title(26))
                    .foregroundStyle(theme.ink)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 32)
                    .padding(.bottom, 8)

                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        Text("Choose a folder on a connected USB drive. Lasco will use only that folder.")
                            .font(LascoFont.body())
                            .foregroundStyle(theme.inkMuted)

                        Button("Choose USB folder", action: chooseFolder)
                            .buttonStyle(LascoSecondaryButtonStyle())
                            .frame(maxWidth: .infinity)

                        if let selectedDriveName {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(selectedDriveName)
                                    .font(LascoFont.body(13))
                                    .foregroundStyle(theme.ink)
                                if let selectedFolderPath {
                                    Text(selectedFolderPath)
                                        .font(LascoFont.body(13))
                                        .foregroundStyle(theme.inkMuted)
                                }
                            }
                            .accessibilityElement(children: .combine)
                        }

                        VStack(alignment: .leading, spacing: 6) {
                            FieldLabel(text: "Remote name")
                            TextField("USB drive", text: $name)
                                .textFieldStyle(.plain)
                                .lascoInput()
                                .autocorrectionDisabled()
                                #if os(iOS)
                                .textInputAutocapitalization(.never)
                                #endif
                        }

                        if let errorMessage {
                            Text(errorMessage)
                                .font(LascoFont.body(13))
                                .foregroundStyle(theme.error)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                    .padding(.horizontal, 32)
                    .padding(.top, 8)
                    .padding(.bottom, 120)
                }
            }

            Button(isAdding ? "Adding…" : "Add Remote", action: addRemote)
                .buttonStyle(LascoPrimaryButtonStyle())
                .frame(maxWidth: .infinity)
                .disabled(!isValid || isAdding)
                .opacity(isValid && !isAdding ? 1 : 0.45)
                .padding(.horizontal, 32)
                .padding(.top, 20)
                .padding(.bottom, 48)
                .background(
                    LinearGradient(
                        colors: [theme.bg.opacity(0), theme.bg],
                        startPoint: .top,
                        endPoint: .bottom
                    )
                )
        }
        .fileImporter(
            isPresented: $showingFolderPicker,
            allowedContentTypes: [.folder],
            allowsMultipleSelection: false,
            onCompletion: saveSelectedFolder
        )
    }

    private func chooseFolder() {
        errorMessage = nil
        showingFolderPicker = true
    }

    private func saveSelectedFolder(_ result: Result<[URL], Error>) {
        do {
            let urls = try result.get()
            guard let url = urls.first else {
                throw LibraryRepositoryError.invalidUsbFolder
            }
            guard url.startAccessingSecurityScopedResource() else {
                throw LibraryRepositoryError.usbAccessDenied
            }
            defer { url.stopAccessingSecurityScopedResource() }

            let folder = try url.resourceValues(forKeys: [
                .isDirectoryKey,
                .volumeURLKey,
            ])
            guard folder.isDirectory == true else {
                throw LibraryRepositoryError.invalidUsbFolder
            }
            guard let volumeURL = folder.volume else {
                throw LibraryRepositoryError.selectedFolderIsNotUsbDrive
            }
            let volume = try volumeURL.resourceValues(forKeys: [
                .volumeIsInternalKey,
                .volumeIsRemovableKey,
                .volumeLocalizedNameKey,
                .volumeNameKey,
            ])
            guard volume.volumeIsRemovable == true, volume.volumeIsInternal != true else {
                throw LibraryRepositoryError.selectedFolderIsNotUsbDrive
            }
            let driveName = volume.volumeLocalizedName ?? volume.volumeName ?? "USB drive"

            bookmarkBase64 = try url
                .bookmarkData(options: [.minimalBookmark], includingResourceValuesForKeys: nil, relativeTo: nil)
                .base64EncodedString()
            selectedDriveName = driveName
            selectedFolderPath = displayFolderPath(url, relativeTo: volumeURL)
            if name.isEmpty || name == suggestedName {
                name = driveName
            }
            suggestedName = driveName
            errorMessage = nil
        } catch is CancellationError {
            return
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    /// Uses File Provider display names instead of URL path components, which
    /// can contain opaque provider identifiers on iOS.
    private func displayFolderPath(_ folderURL: URL, relativeTo volumeURL: URL) -> String? {
        let volumePath = volumeURL.standardizedFileURL.path
        var current = folderURL.standardizedFileURL
        var names: [String] = []

        while current.path != volumePath {
            guard let values = try? current.resourceValues(forKeys: [.nameKey]),
                  let displayName = values.name,
                  !displayName.isEmpty else {
                return nil
            }
            names.append(displayName)

            let parent = current.deletingLastPathComponent().standardizedFileURL
            guard parent.path != current.path else { return nil }
            current = parent
        }

        return names.isEmpty ? nil : names.reversed().joined(separator: "/")
    }

    private func addRemote() {
        guard let bookmarkBase64, !isAdding else { return }
        let remoteName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        isAdding = true
        errorMessage = nil

        Task {
            var addedRemoteID: FfiRemoteUuid?
            do {
                try await repository.ensureUsbAppleFolderIsUninitialized(
                    bookmarkBase64: bookmarkBase64
                )
                let remoteID = try await repository.addRemoteUsbApple(
                    name: remoteName,
                    bookmarkBase64: bookmarkBase64
                )
                addedRemoteID = remoteID
                try await repository.initializeRemote(id: remoteID)
                try await onRemoteReady()
                dismiss()
                toastManager.show(ok: "\(remoteName): initialized")
            } catch {
                if let addedRemoteID {
                    try? await repository.removeRemote(id: addedRemoteID)
                }
                errorMessage = error.localizedDescription
            }
            isAdding = false
        }
    }
}
