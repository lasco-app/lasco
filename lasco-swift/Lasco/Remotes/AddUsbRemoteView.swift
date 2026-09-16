import SwiftUI
import UniformTypeIdentifiers

struct AddUsbRemoteView: View {
    @Environment(LibraryRepository.self) private var repository
    @Environment(ToastManager.self) private var toastManager
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) private var theme

    @State private var name = ""
    @State private var bookmarkBase64: String?
    @State private var selectedFolderName: String?
    @State private var showingFolderPicker = false
    @State private var isAdding = false
    @State private var errorMessage: String?

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
                        Text("Connect the drive, then choose a folder on it. Lasco will use only that folder.")
                            .font(LascoFont.body())
                            .foregroundStyle(theme.inkMuted)

                        Button(selectedFolderName ?? "Choose USB folder", action: chooseFolder)
                            .buttonStyle(LascoSecondaryButtonStyle())
                            .frame(maxWidth: .infinity)

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
            guard let url = urls.first, url.hasDirectoryPath else {
                throw LibraryRepositoryError.invalidUsbFolder
            }
            guard url.startAccessingSecurityScopedResource() else {
                throw LibraryRepositoryError.usbAccessDenied
            }
            defer { url.stopAccessingSecurityScopedResource() }

            bookmarkBase64 = try url
                .bookmarkData(options: [.minimalBookmark], includingResourceValuesForKeys: nil, relativeTo: nil)
                .base64EncodedString()
            selectedFolderName = url.lastPathComponent
            if name.isEmpty {
                name = url.lastPathComponent
            }
            errorMessage = nil
        } catch is CancellationError {
            return
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func addRemote() {
        guard let bookmarkBase64, !isAdding else { return }
        let remoteName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        isAdding = true
        errorMessage = nil

        Task {
            var addedRemoteID: FfiRemoteUuid?
            do {
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
