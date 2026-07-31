import SwiftUI
#if os(iOS)
import Photos
#endif

struct NewLibraryWizard: View {
    @Environment(LibraryDirectoryModel.self) private var directory
    @AppStorage("expertMode") private var expertMode = false

    var onBack: () -> Void
    var onComplete: () -> Void

    @State private var step: Int
    @State private var slideForward = true
    @State private var name = ""
    @State private var username = ""
    @State private var password = ""
    @State private var confirmPassword = ""
    @State private var showAddS3Sheet = false
    @State private var showAddLocalFSSheet = false
    @State private var masterKeyCopied = false
    @State private var masterKey: String?

    init(initialStep: Int = 0, onBack: @escaping () -> Void, onComplete: @escaping () -> Void) {
        _step = State(initialValue: initialStep)
        self.onBack = onBack
        self.onComplete = onComplete
    }

    #if os(iOS)
    @Environment(\.scenePhase) private var scenePhase
    @State private var initialImportController: InitialPhotoImportController?
    @State private var photoPermissionDenied = false
    @State private var showIgnoredDetail = false
    #endif

    private var pageSlide: AnyTransition {
        .asymmetric(
            insertion: .move(edge: slideForward ? .trailing : .leading),
            removal: .move(edge: slideForward ? .leading : .trailing)
        )
    }

    #if os(iOS)
    private var totalSteps: Int { 7 }
    #else
    private var totalSteps: Int { 3 }
    #endif

    private var canCreate: Bool {
        !name.isEmpty && !username.isEmpty && password.count >= 5 && password == confirmPassword
    }

    var body: some View {
        ZStack(alignment: .bottom) {
            Color.Lasco.bg.ignoresSafeArea()

            VStack(spacing: 0) {
                topBar(dots: totalSteps, current: step, back: backAction)

                logo()

                ZStack {
                    Group {
                        switch step {
                        case 0: createStep
                        case 1: masterKeyStep
                        case 2: remoteStep
                        #if os(iOS)
                        case 3: askImportStep
                        case 4: permissionStep
                        case 5: importOrSuccessStep
                        default: autoImportStep
                        #else
                        default: remoteStep
                        #endif
                        }
                    }
                    .id(step)
                    .transition(pageSlide)
                }
                .clipped()
            }

            bottomBar {
                stepButtons
            }
        }
        .onChange(of: step) { _, newValue in
            if let libraryID = directory.activeSession?.state.libraryID {
                directory.setOnboardingStep(newValue, libraryID: libraryID)
            }
        }
        .onDisappear {
            Task { await initialImportController?.cancelAndWait() }
        }
        #if os(iOS)
        .onChange(of: scenePhase) { _, newPhase in
            if newPhase == .active && step == 4 && photoPermissionDenied {
                requestPhotoPermission()
            }
        }
        #endif
    }

    // MARK: - Step buttons

    @ViewBuilder
    private var stepButtons: some View {
        if step == 0 {
            Button("Create Library") {
                directory.onboarding.clearError()
                Task {
                    do {
                        let result = try await directory.create(name: name, username: username, password: password)
                        masterKey = result.masterKey
                        slideForward = true
                        withAnimation(.easeInOut(duration: 0.3)) { step = 1 }
                    } catch {
                        directory.onboarding.setError(error)
                    }
                }
            }
            .buttonStyle(LascoPrimaryButtonStyle())
            .frame(maxWidth: .infinity)
            .disabled(!canCreate)
            .opacity(canCreate ? 1 : 0.45)
        } else if step == 1 {
            Button("I've saved my key") {
                masterKey = nil
                slideForward = true
                withAnimation(.easeInOut(duration: 0.3)) { step = 2 }
            }
            .buttonStyle(LascoPrimaryButtonStyle())
            .frame(maxWidth: .infinity)
        } else if step == 2 {
            VStack(spacing: 12) {
                Button("Add S3-compatible remote") { showAddS3Sheet = true }
                    .buttonStyle(LascoPrimaryButtonStyle())
                    .frame(maxWidth: .infinity)
                if expertMode {
                    Button("Add local filesystem remote") { showAddLocalFSSheet = true }
                        .buttonStyle(LascoDevButtonStyle())
                        .frame(maxWidth: .infinity)
                }
                Button("Skip for now") { advanceFromRemote() }
                    .buttonStyle(LascoSecondaryButtonStyle())
                    .frame(maxWidth: .infinity)
            }
            .sheet(isPresented: $showAddS3Sheet) {
                if let activeSession = directory.activeSession {
                    AddS3RemoteView {
                        try await activeSession.refresh()
                        guard !activeSession.state.remotes.isEmpty else {
                            throw LibraryDirectoryModelError.remoteUnavailableAfterRefresh
                        }
                        advanceFromRemote()
                    }
                    .environment(activeSession.repository)
                }
            }
            .sheet(isPresented: $showAddLocalFSSheet) {
                if let activeSession = directory.activeSession {
                    AddLocalFSRemoteView {
                        try await activeSession.refresh()
                        guard !activeSession.state.remotes.isEmpty else {
                            throw LibraryDirectoryModelError.remoteUnavailableAfterRefresh
                        }
                        advanceFromRemote()
                    }
                    .environment(activeSession.repository)
                }
            }
        } else {
            #if os(iOS)
            if step == 3 {
                askImportButtons
            } else if step == 4 {
                permissionButtons
            } else if step == 5 {
                importOrSuccessButtons
            } else {
                autoImportButtons
            }
            #endif
        }
    }

    private func finish() {
        if let libraryID = directory.activeSession?.state.libraryID {
            directory.clearOnboardingIncomplete(libraryID: libraryID)
        }
        directory.completeOnboarding()
        onComplete()
    }

    private func advanceFromRemote() {
        #if os(iOS)
        slideForward = true
        withAnimation(.easeInOut(duration: 0.3)) { step = 3 }
        #else
        finish()
        #endif
    }

    private var backAction: (() -> Void)? {
        if step == 0 {
            return {
                slideForward = false
                onBack()
            }
        }
        #if os(iOS)
        if step == 3 && (directory.activeSession?.state.remotes ?? []).isEmpty {
            return {
                slideForward = false
                withAnimation(.easeInOut(duration: 0.3)) { step = 2 }
            }
        }
        if step == 4 {
            return {
                slideForward = false
                withAnimation(.easeInOut(duration: 0.3)) { step = 3 }
            }
        }
        #endif
        return nil
    }

    // MARK: - Chrome

    private func topBar(dots: Int, current: Int, back: (() -> Void)?) -> some View {
        HStack {
            if let back {
                Button(action: back) {
                    Text("← Back")
                        .font(LascoFont.body(14))
                        .foregroundStyle(Color.Lasco.inkMuted)
                }
                .buttonStyle(.plain)
            } else {
                Spacer().frame(width: 60)
            }

            Spacer()

            HStack(spacing: 8) {
                ForEach(0..<dots, id: \.self) { i in
                    Rectangle()
                        .fill(i == current ? Color.Lasco.ink : Color.Lasco.inkMuted.opacity(0.35))
                        .frame(width: i == current ? 20 : 8, height: 3)
                        .animation(.easeInOut(duration: 0.2), value: current)
                }
            }
        }
        .padding(.horizontal, 32)
        .padding(.top, 32)
        .padding(.bottom, 16)
    }

    private func logo() -> some View {
        Text("LASCO")
            .font(LascoFont.categoryLarge(28))
            .foregroundStyle(Color.Lasco.ink)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 32)
            .padding(.bottom, 8)
    }

    private func bottomBar<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        VStack(content: content)
            .padding(.horizontal, 32)
            .padding(.top, 20)
            .padding(.bottom, 48)
            .background(
                LinearGradient(
                    colors: [Color.Lasco.bg.opacity(0), Color.Lasco.bg],
                    startPoint: .top, endPoint: .bottom
                )
            )
    }

    // MARK: - Steps

    private var createStep: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Create your library.")
                .font(LascoFont.title(26))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)

            Text("Your library is encrypted locally. Choose a strong password.")
                .font(LascoFont.body(16))
                .foregroundStyle(Color.Lasco.inkSub)
                .fixedSize(horizontal: false, vertical: true)
                .lineSpacing(4)

            LibraryCreateForm(
                name: $name,
                username: $username,
                password: $password,
                confirmPassword: $confirmPassword,
                error: directory.onboarding.error
            )

            Spacer()
        }
        .padding(.horizontal, 32)
        .padding(.top, 40)
        .padding(.bottom, 120)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var masterKeyStep: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Save your master key.")
                .font(LascoFont.title(26))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)

            Text("This key can restore your library if you forget your password. Store it somewhere safe.")
                .font(LascoFont.body(16))
                .foregroundStyle(Color.Lasco.inkSub)
                .fixedSize(horizontal: false, vertical: true)
                .lineSpacing(4)

            if let key = masterKey {
                VStack(alignment: .leading, spacing: 0) {
                    HStack {
                        Text(key)
                            .font(LascoFont.mono(13))
                            .foregroundStyle(Color.Lasco.inkSub)
                            .lineLimit(nil)
                            .fixedSize(horizontal: false, vertical: true)
                        Spacer()
                        Button {
                            #if os(iOS)
                            UIPasteboard.general.string = key
                            #else
                            NSPasteboard.general.clearContents()
                            NSPasteboard.general.setString(key, forType: .string)
                            #endif
                            masterKeyCopied = true
                        } label: {
                            Image(systemName: "doc.on.doc")
                                .foregroundStyle(Color.Lasco.inkMuted)
                        }
                        .buttonStyle(.plain)
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
                }
                .lascoPanel()

                if masterKeyCopied {
                    Text("Master key copied")
                        .font(LascoFont.body(13))
                        .foregroundStyle(Color.Lasco.inkSub)
                }
            }

            Spacer()
        }
        .padding(.horizontal, 32)
        .padding(.top, 40)
        .padding(.bottom, 120)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var remoteStep: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Add your first remote.")
                .font(LascoFont.title(26))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)

            Text("Connect a destination to store your photos.")
                .font(LascoFont.body(16))
                .foregroundStyle(Color.Lasco.inkSub)
                .fixedSize(horizontal: false, vertical: true)
                .lineSpacing(4)

            Text("You can add another remote later.")
                .font(LascoFont.body(16))
                .foregroundStyle(Color.Lasco.inkSub)
                .fixedSize(horizontal: false, vertical: true)
                .lineSpacing(4)

            Spacer()
        }
        .padding(.horizontal, 32)
        .padding(.top, 40)
        .padding(.bottom, 120)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    // MARK: - Import steps (iOS only)

    #if os(iOS)
    private var askImportStep: some View {
        let hasRemote = !(directory.activeSession?.state.remotes ?? []).isEmpty

        return VStack(alignment: .leading, spacing: 20) {
            Text(hasRemote ? "Import your device photos?" : "Can't import your current photo library yet.")
                .font(LascoFont.title(26))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)

            if !hasRemote {
                Text("Because there is no remote yet, it would mean that everything should be saved twice locally on your device.")
                    .font(LascoFont.body(16))
                    .foregroundStyle(Color.Lasco.inkSub)
                    .fixedSize(horizontal: false, vertical: true)
                    .lineSpacing(4)
            } else {
                Text("Lasco can import your existing photos and videos and back them up to your remote.")
                    .font(LascoFont.body(16))
                    .foregroundStyle(Color.Lasco.inkSub)
                    .fixedSize(horizontal: false, vertical: true)
                    .lineSpacing(4)

                Text("Nothing is deleted from your device.")
                    .font(LascoFont.body(16))
                    .foregroundStyle(Color.Lasco.inkSub)
                    .fixedSize(horizontal: false, vertical: true)
                    .lineSpacing(4)
            }

            Spacer()
        }
        .padding(.horizontal, 32)
        .padding(.top, 40)
        .padding(.bottom, 120)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var askImportButtons: some View {
        Group {
            if (directory.activeSession?.state.remotes ?? []).isEmpty {
                Button("Get started") { finish() }
                    .buttonStyle(LascoPrimaryButtonStyle())
                    .frame(maxWidth: .infinity)
            } else {
                VStack(spacing: 12) {
                    Button("Yes, import my photos") {
                        slideForward = true
                        withAnimation(.easeInOut(duration: 0.3)) { step = 4 }
                    }
                    .buttonStyle(LascoPrimaryButtonStyle())
                    .frame(maxWidth: .infinity)

                    Button("No, not now") {
                        slideForward = true
                        withAnimation(.easeInOut(duration: 0.3)) { step = 6 }
                    }
                    .buttonStyle(LascoSecondaryButtonStyle())
                    .frame(maxWidth: .infinity)
                }
            }
        }
    }

    private var permissionStep: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Access your photos.")
                .font(LascoFont.title(26))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)

            Text("We'll ask for permission to access your photos so Lasco can import them.")
                .font(LascoFont.body(16))
                .foregroundStyle(Color.Lasco.inkSub)
                .fixedSize(horizontal: false, vertical: true)
                .lineSpacing(4)

            if photoPermissionDenied {
                Text("Access was denied. You can grant it in Settings and come back here.")
                    .font(LascoFont.body(16))
                    .foregroundStyle(Color.Lasco.inkSub)
                    .fixedSize(horizontal: false, vertical: true)
                    .lineSpacing(4)
            }

            Spacer()
        }
        .padding(.horizontal, 32)
        .padding(.top, 40)
        .padding(.bottom, 120)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var permissionButtons: some View {
        Group {
            if photoPermissionDenied {
                VStack(spacing: 12) {
                    Button("Open Settings") {
                        if let url = URL(string: UIApplication.openSettingsURLString) {
                            UIApplication.shared.open(url)
                        }
                    }
                    .buttonStyle(LascoPrimaryButtonStyle())
                    .frame(maxWidth: .infinity)

                    Button("Skip for now") {
                        slideForward = true
                        withAnimation(.easeInOut(duration: 0.3)) { step = 6 }
                    }
                    .buttonStyle(LascoSecondaryButtonStyle())
                    .frame(maxWidth: .infinity)
                }
            } else {
                Button("Continue") { requestPhotoPermission() }
                    .buttonStyle(LascoPrimaryButtonStyle())
                    .frame(maxWidth: .infinity)
            }
        }
    }

    private func requestPhotoPermission() {
        Task {
            let status = await PHPhotoLibrary.requestAuthorization(for: .readWrite)
            if status == .authorized {
                photoPermissionDenied = false
                slideForward = true
                withAnimation(.easeInOut(duration: 0.3)) { step = 5 }
            } else {
                photoPermissionDenied = true
            }
        }
    }

    private var importOrSuccessStep: some View {
        if let result = initialImportController?.result {
            return AnyView(importSuccessStep(photos: result.photos, videos: result.videos))
        }
        return AnyView(importStep)
    }

    private var importOrSuccessButtons: some View {
        if initialImportController?.result != nil {
            return AnyView(
                Button("Continue") {
                    slideForward = true
                    withAnimation(.easeInOut(duration: 0.3)) { step = 6 }
                }
                .buttonStyle(LascoPrimaryButtonStyle())
                .frame(maxWidth: .infinity)
            )
        }
        return AnyView(importButtons)
    }

    private var importStep: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Import your photo library?")
                .font(LascoFont.title(26))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)

            Text("Lasco will import your existing photos and videos and back them up to your remote. No copy is kept on this device.")
                .font(LascoFont.body(16))
                .foregroundStyle(Color.Lasco.inkSub)
                .fixedSize(horizontal: false, vertical: true)
                .lineSpacing(4)

            if initialImportController?.isScanning == true {
                HStack(spacing: 12) {
                    ProgressView()
                        .tint(Color.Lasco.inkMuted)
                    Text("Scanning library…")
                        .font(LascoFont.body(14))
                        .foregroundStyle(Color.Lasco.inkMuted)
                }
            } else if let scan = initialImportController?.scan {
                VStack(alignment: .leading, spacing: 0) {
                    importStatRow(label: "Photos", value: "\(scan.photoCount)")
                    importStatRow(label: "Videos", value: "\(scan.videoCount)")
                    importStatRow(label: "Video from live photo", value: "\(scan.livePhotoVideoCount)")
                    importStatRow(label: "Photo edit metadata", value: "\(scan.editMetadataCount)")
                    if !scan.ignoredAssets.isEmpty {
                        Button { showIgnoredDetail = true } label: {
                            importStatRow(label: "Ignored", value: "\(scan.ignoredAssets.count)")
                        }
                        .buttonStyle(.plain)
                    }
                }
                .lascoPanel()
            }

            Spacer()
        }
        .padding(.horizontal, 32)
        .padding(.top, 40)
        .padding(.bottom, 160)
        .frame(maxWidth: .infinity, alignment: .leading)
        .task {
            await prepareInitialPhotoImport()
        }
        .sheet(isPresented: $showIgnoredDetail) {
            if let scan = initialImportController?.scan {
                IgnoredAssetsView(ignoredAssets: scan.ignoredAssets)
            }
        }
    }

    private var importButtons: some View {
        VStack(spacing: 12) {
            if initialImportController?.isImporting == true {
                let progress = initialImportController?.progress
                let done = progress?.done ?? 0
                let total = max(progress?.total ?? 1, 1)

                VStack(spacing: 8) {
                    ProgressView(value: Double(done), total: Double(total))
                        .tint(Color.Lasco.ink)
                        .frame(maxWidth: .infinity)
                    Text("\(done) of \(total)")
                        .font(LascoFont.mono(13))
                        .foregroundStyle(Color.Lasco.inkMuted)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                }
                .padding(.vertical, 8)
            } else {
                Button("Import Now") {
                    if let controller = initialImportController {
                        Task {
                            await controller.start(remoteID: directory.activeSession?.state.remotes.first?.id)
                        }
                    }
                }
                .buttonStyle(LascoPrimaryButtonStyle())
                .frame(maxWidth: .infinity)
                .disabled(initialImportController?.scan == nil)
                .opacity(initialImportController?.scan == nil ? 0.45 : 1)

                if let error = initialImportController?.error {
                    Text(error)
                        .font(LascoFont.body(13))
                        .foregroundStyle(Color.Lasco.pink)
                }

                Button("Skip for now") {
                    slideForward = true
                    withAnimation(.easeInOut(duration: 0.3)) { step = 6 }
                }
                .buttonStyle(LascoSecondaryButtonStyle())
                .frame(maxWidth: .infinity)
            }
        }
    }

    private var autoImportStep: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Automatically import new photos?")
                .font(LascoFont.title(26))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)

            Text("Lasco can check for new photos each time you open the app and import any taken after this point.")
                .font(LascoFont.body(16))
                .foregroundStyle(Color.Lasco.inkSub)
                .fixedSize(horizontal: false, vertical: true)
                .lineSpacing(4)

            Spacer()
        }
        .padding(.horizontal, 32)
        .padding(.top, 40)
        .padding(.bottom, 120)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var autoImportButtons: some View {
        VStack(spacing: 12) {
            Button("Yes, auto-import new photos") {
                Task {
                    if let activeSession = directory.activeSession {
                        try? await activeSession.repository.setAutoImportDeviceMedia(enabled: true)
                    }
                    finish()
                }
            }
            .buttonStyle(LascoPrimaryButtonStyle())
            .frame(maxWidth: .infinity)

            Button("No, not now") {
                Task {
                    if let activeSession = directory.activeSession {
                        try? await activeSession.repository.setAutoImportDeviceMedia(enabled: false)
                    }
                    finish()
                }
            }
            .buttonStyle(LascoSecondaryButtonStyle())
            .frame(maxWidth: .infinity)
        }
    }

    private func importSuccessStep(photos: Int, videos: Int) -> some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("All done.")
                .font(LascoFont.title(26))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)

            Text("\(photos) \(photos == 1 ? "photo" : "photos") and \(videos) \(videos == 1 ? "video" : "videos") were successfully imported.")
                .font(LascoFont.body(16))
                .foregroundStyle(Color.Lasco.inkSub)
                .fixedSize(horizontal: false, vertical: true)
                .lineSpacing(4)

            Spacer()
        }
        .padding(.horizontal, 32)
        .padding(.top, 40)
        .padding(.bottom, 160)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func importStatRow(label: String, value: String) -> some View {
        HStack {
            Text(label)
                .font(LascoFont.body())
                .foregroundStyle(Color.Lasco.inkSub)
            Spacer()
            Text(value)
                .font(LascoFont.mono())
                .foregroundStyle(Color.Lasco.inkMuted)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }

    private func prepareInitialPhotoImport() async {
        guard let activeSession = directory.activeSession else { return }
        try? await activeSession.refresh()
        guard initialImportController == nil,
              let defaultAlbumID = activeSession.state.defaultUploadAlbumID else { return }
        let controller = InitialPhotoImportController(
            repository: activeSession.repository,
            defaultUploadAlbumID: defaultAlbumID,
            pushChunk: { remoteID in await activeSession.syncCoordinator.push(remoteID: remoteID) }
        )
        initialImportController = controller
        await controller.scanPhotoLibrary()
    }
    #endif
}
