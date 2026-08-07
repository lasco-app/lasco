import SwiftUI

extension FfiLibraryEntry: Identifiable {
    public var id: FfiLibraryId { libraryId }
}

struct LibraryListView: View {
    @Environment(LibraryDirectoryModel.self) private var directory
    @Environment(ToastManager.self) var toastManager

    @State private var selectedEntry: FfiLibraryEntry?
    @State private var showSettings = false
    @State private var showNewLibrary = false
    @State private var showAddExisting = false

    var body: some View {
        ZStack {
            Color.Lasco.bg.ignoresSafeArea()

            VStack(alignment: .leading, spacing: 0) {
                HStack(alignment: .top) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("LASCO")
                            .font(LascoFont.categoryLarge())
                            .foregroundStyle(Color.Lasco.ink)
                        Text("Your libraries")
                            .font(LascoFont.subtitle())
                            .foregroundStyle(Color.Lasco.inkMuted)
                    }
                    Spacer()
                    Button { showSettings = true } label: {
                        Image("cog").renderingMode(.template).resizable().frame(width: 18, height: 18)
                            .font(.system(size: 18))
                            .foregroundStyle(Color.Lasco.inkMuted)
                    }
                    .buttonStyle(.plain)
                    .padding(.top, 6)
                }
                .padding(.horizontal, 32)
                .padding(.top, 48)
                .padding(.bottom, 32)

                ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    if let err = directory.librariesError {
                        VStack(alignment: .leading, spacing: 6) {
                            Text("Could not load libraries")
                                .font(LascoFont.body())
                                .foregroundStyle(Color.Lasco.ink)
                            Text(err)
                                .font(LascoFont.mono())
                                .foregroundStyle(Color.Lasco.inkMuted)
                                .lineLimit(4)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 16)
                        .lascoPanel()
                    } else if directory.libraries.isEmpty {
                        Text("No libraries yet.")
                            .font(LascoFont.body())
                            .foregroundStyle(Color.Lasco.inkMuted)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 20)
                            .lascoPanel()
                    } else {
                        ForEach(directory.libraries, id: \.id) { entry in
                            if let err = entry.loadError {
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(entry.id.value.isEmpty ? "Unknown library" : entry.nickname)
                                        .font(LascoFont.body())
                                        .foregroundStyle(Color.Lasco.ink)
                                    Text(err)
                                        .font(LascoFont.mono())
                                        .foregroundStyle(Color.Lasco.inkMuted)
                                        .lineLimit(3)
                                }
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.horizontal, 16)
                                .padding(.vertical, 14)
                                .lascoPanel()
                            } else {
                                Button {
                                    Task {
                                        if await directory.openCached(entry: entry) == false {
                                            selectedEntry = entry
                                        }
                                    }
                                } label: {
                                    Text(entry.nickname)
                                        .font(LascoFont.body())
                                        .foregroundStyle(Color.Lasco.inkSub)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                        .padding(.horizontal, 16)
                                        .padding(.vertical, 14)
                                        .lascoPanelHard()
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
                .padding(.horizontal, 32)
                } // ScrollView

                Spacer()

                VStack(spacing: 12) {
                    Button("New library") {
                        showNewLibrary = true
                    }
                    .buttonStyle(LascoPrimaryButtonStyle())
                    .frame(maxWidth: .infinity)

                    Button("Add existing library") {
                        showAddExisting = true
                    }
                    .buttonStyle(.plain)
                    .font(LascoFont.body(15))
                    .foregroundStyle(Color.Lasco.inkMuted)
                }
                .padding(.horizontal, 32)
                .padding(.bottom, 48)
            }
        }
        .sheet(item: $selectedEntry) { entry in
            LibraryOpenSheet(entry: entry)
                .environment(directory)
        }
        .sheet(isPresented: $showSettings) {
            SettingsView()
        }
        .sheet(isPresented: $showNewLibrary) {
            NewLibraryWizard(
                onBack: { showNewLibrary = false },
                onComplete: { showNewLibrary = false }
            )
            .environment(directory)
        }
        .sheet(isPresented: $showAddExisting) {
            AddExistingLibraryView()
                .environment(directory)
                .environment(toastManager)
        }
    }
}

struct LibraryOpenSheet: View {
    @Environment(LibraryDirectoryModel.self) private var directory
    @Environment(\.dismiss) private var dismiss

    let entry: FfiLibraryEntry

    @State private var username: String
    @State private var password = ""
    @State private var isLoading = false
    @FocusState private var passwordFocused: Bool

    init(entry: FfiLibraryEntry) {
        self.entry = entry
        _username = State(initialValue: entry.username ?? "")
    }

    private var canSubmit: Bool { !username.isEmpty && !password.isEmpty && !isLoading }

    var body: some View {
        ZStack {
            Color.Lasco.bg.ignoresSafeArea()

            VStack(alignment: .leading, spacing: 24) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(entry.nickname)
                        .font(LascoFont.title())
                        .foregroundStyle(Color.Lasco.ink)
                    Text("Enter credentials to open")
                        .font(LascoFont.subtitle())
                        .foregroundStyle(Color.Lasco.inkMuted)
                }

                VStack(alignment: .leading, spacing: 16) {
                    VStack(alignment: .leading, spacing: 6) {
                        FieldLabel(text: "Username")
                        TextField("", text: $username)
                            .textFieldStyle(.plain)
                            .lascoInput()
                            .autocorrectionDisabled()
                            .disabled(entry.username != nil)
                            .opacity(entry.username != nil ? 0.6 : 1)
                    }

                    VStack(alignment: .leading, spacing: 6) {
                        FieldLabel(text: "Password")
                        SecureField("", text: $password)
                            .textFieldStyle(.plain)
                            .lascoInput()
                            .focused($passwordFocused)
                    }

                    if let error = directory.onboarding.error {
                        ErrorBanner(message: error)
                    }

                    Button {
                        isLoading = true
                        Task {
                            _ = await directory.open(nickname: entry.nickname, username: username, password: password)
                            isLoading = false
                            if directory.isOpen { dismiss() }
                        }
                    } label: {
                        if isLoading {
                            HStack(spacing: 8) {
                                ProgressView().tint(Color.Lasco.surface)
                                Text("Opening…")
                            }
                        } else {
                            Text("Open Library")
                        }
                    }
                    .buttonStyle(LascoPrimaryButtonStyle())
                    .disabled(!canSubmit)
                    .opacity(canSubmit ? 1 : 0.45)
                }
                .padding(24)
                .lascoPanelHard()
            }
            .padding(32)
        }
        .onAppear {
            if entry.username != nil {
                passwordFocused = true
            }
        }
    }
}

#Preview {
    LibraryListView()
}
