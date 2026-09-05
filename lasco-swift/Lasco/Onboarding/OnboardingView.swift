import SwiftUI

struct OnboardingView: View {
    @Environment(LibraryDirectoryModel.self) private var directory
    @Environment(ToastManager.self) var toastManager

    private enum Flow { case main, cloud, own, existingChoice }

    @State private var page = 0
    @State private var flow: Flow = .main
    @State private var wizardInitialStep = 0
    @State private var slideForward = true
    @State private var encryptedMascot: Image? = nil
    @State private var betaMascot: Image? = nil

    private var pageSlide: AnyTransition {
        .asymmetric(
            insertion: .move(edge: slideForward ? .trailing : .leading),
            removal:   .move(edge: slideForward ? .leading  : .trailing)
        )
    }

    // Cloud fields
    @State private var cloudEmail = ""
    @State private var cloudPassword = ""
    @State private var cloudLibraryName = ""
    @State private var cloudStep = 0

    @State private var existingLibrarySource: ExistingLibrarySource?
    @State private var libraryCountBeforeAddExisting = 0

    var body: some View {
        ZStack {
            Color.Lasco.bg.ignoresSafeArea()

            Group {
                switch flow {
                case .main:           mainBody
                case .cloud:          cloudBody
                case .own:
                    NewLibraryWizard(
                        initialStep: wizardInitialStep,
                        onBack: {
                            slideForward = false
                            withAnimation(.easeInOut(duration: 0.3)) { flow = .main }
                        },
                        onComplete: { directory.showOnboarding = false }
                    )
                    .environment(directory)
                case .existingChoice: existingChoiceBody
                }
            }
            .id(flow)
            .transition(pageSlide)
            .clipped()
        }
        .onAppear { resumeOnboardingIfNeeded() }
        .task {
            await loadMascots()
        }
    }

    private func resumeOnboardingIfNeeded() {
        guard directory.onboarding.resumeLibraryID != nil else { return }
        directory.onboarding.resumeLibraryID = nil
        wizardInitialStep = directory.onboarding.resumeStep
        flow = .own
    }

    private func loadMascots() async {
        async let encrypted = fetchImage("https://public.getlasco.app/mascot_encrypted_0_5x.png")
        async let beta = fetchImage("https://public.getlasco.app/mascot_beta_0_5x.png")
        encryptedMascot = await encrypted
        betaMascot = await beta
    }

    private func fetchImage(_ urlString: String) async -> Image? {
        guard let url = URL(string: urlString),
              let (data, _) = try? await URLSession.shared.data(from: url) else { return nil }
        #if canImport(UIKit)
        guard let uiImage = UIImage(data: data) else { return nil }
        return Image(uiImage: uiImage)
        #else
        guard let nsImage = NSImage(data: data) else { return nil }
        return Image(nsImage: nsImage)
        #endif
    }

    // MARK: - Shared chrome

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

    // MARK: - Main flow (pages 0–3)

    private var mainBody: some View {
        ZStack(alignment: .bottom) {
            Color.Lasco.bg.ignoresSafeArea()

            VStack(spacing: 0) {
                HStack(spacing: 8) {
                    ForEach(0..<4, id: \.self) { i in
                        Rectangle()
                            .fill(i == page ? Color.Lasco.ink : Color.Lasco.inkMuted.opacity(0.35))
                            .frame(width: i == page ? 20 : 8, height: 3)
                            .animation(.easeInOut(duration: 0.2), value: page)
                    }
                    Spacer()
                    Button("Skip") {
                        directory.showOnboarding = false
                    }
                        .font(LascoFont.body(14))
                        .foregroundStyle(Color.Lasco.inkMuted)
                        .buttonStyle(.plain)
                }
                .padding(.horizontal, 32)
                .padding(.top, 32)
                .padding(.bottom, 16)

                logo()

                ZStack {
                    Group {
                        switch page {
                        case 0:
                            syncInfoPage
                        case 1:
                            encryptedPage
                        case 2:
                            betaPage
                        default:
                            existingLibraryPage
                        }
                    }
                    .id(page)
                    .transition(pageSlide)
                }
                .clipped()
            }

            if page < 3 {
                bottomBar {
                    Button("Next") {
                        slideForward = true
                        withAnimation(.easeInOut(duration: 0.3)) { page = min(page + 1, 3) }
                    }
                    .buttonStyle(LascoPrimaryButtonStyle())
                    .frame(maxWidth: .infinity)
                }
                .transition(.opacity)
            }
        }
    }

    // MARK: - Cloud flow

    private var cloudBody: some View {
        ZStack(alignment: .bottom) {
            Color.Lasco.bg.ignoresSafeArea()

            VStack(spacing: 0) {
                topBar(dots: 3, current: cloudStep, back: {
                    slideForward = false
                    withAnimation(.easeInOut(duration: 0.3)) {
                        if cloudStep > 0 { cloudStep -= 1 } else { flow = .main }
                    }
                })

                logo()

                ZStack {
                    Group {
                        switch cloudStep {
                        case 0: cloudAccountStep
                        case 1: cloudPlanStep
                        default: cloudLibraryStep
                        }
                    }
                    .id(cloudStep)
                    .transition(pageSlide)
                }
                .clipped()
            }

            bottomBar {
                if cloudStep == 0 {
                    Button("Continue") {
                        slideForward = true
                        withAnimation(.easeInOut(duration: 0.3)) { cloudStep = 1 }
                    }
                    .buttonStyle(LascoPrimaryButtonStyle())
                    .frame(maxWidth: .infinity)
                    .disabled(cloudEmail.isEmpty || cloudPassword.isEmpty)
                    .opacity(cloudEmail.isEmpty || cloudPassword.isEmpty ? 0.45 : 1)
                } else if cloudStep == 1 {
                    Button("Continue") {
                        slideForward = true
                        withAnimation(.easeInOut(duration: 0.3)) { cloudStep = 2 }
                    }
                    .buttonStyle(LascoPrimaryButtonStyle())
                    .frame(maxWidth: .infinity)
                } else {
                    Button("Get started") { directory.showOnboarding = false }
                        .buttonStyle(LascoPrimaryButtonStyle())
                        .frame(maxWidth: .infinity)
                        .disabled(cloudLibraryName.isEmpty)
                        .opacity(cloudLibraryName.isEmpty ? 0.45 : 1)
                }
            }
        }
    }

    // MARK: - Existing library flow

    private var existingChoiceBody: some View {
        ZStack(alignment: .bottom) {
            Color.Lasco.bg.ignoresSafeArea()

            VStack(spacing: 0) {
                topBar(dots: 1, current: 0, back: {
                    slideForward = false
                    withAnimation(.easeInOut(duration: 0.3)) { flow = .main }
                })

                logo()

                VStack(alignment: .leading, spacing: 20) {
                    Text("Where is your library?")
                        .font(LascoFont.title(26))
                        .foregroundStyle(Color.Lasco.ink)
                        .fixedSize(horizontal: false, vertical: true)

                    Spacer()
                }
                .padding(.horizontal, 32)
                .padding(.top, 40)
                .padding(.bottom, 120)
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            bottomBar {
                Button("S3-compatible storage") {
                    libraryCountBeforeAddExisting = directory.libraries.count
                    existingLibrarySource = .s3
                }
                    .buttonStyle(LascoPrimaryButtonStyle())
                    .frame(maxWidth: .infinity)

                Button("Lasco Cloud") {
                    libraryCountBeforeAddExisting = directory.libraries.count
                    existingLibrarySource = .lascoCloud
                }
                .buttonStyle(LascoSecondaryButtonStyle())
                .frame(maxWidth: .infinity)
            }
        }
        .sheet(item: $existingLibrarySource, onDismiss: {
            if directory.libraries.count > libraryCountBeforeAddExisting { directory.showOnboarding = false }
        }) { source in
            AddExistingLibraryView(source: source)
                .environment(directory)
                .environment(toastManager)
        }
    }

    // MARK: - Main pages

    private func infoPage(title: String, body bodyText: String) -> some View {
        VStack(alignment: .leading, spacing: 20) {
            Text(title)
                .font(LascoFont.title(26))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)

            Text(bodyText)
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

    private var syncInfoPage: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Sync your photo library to S3 file storage.")
                .font(LascoFont.title(26))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)

            VStack(alignment: .leading, spacing: 16) {
                featureRow(
                    title: "No server to self-host",
                    body: "Syncs directly to **your S3**. No backend to deploy. No data sent to us."
                )
                featureRow(
                    title: "Multi-device",
                    body: "Lasco uses CRDT algorithms, the standard for local-first sync, so edits from every device merge nicely."
                )
                featureRow(
                    title: "E2EE",
                    body: "Your photos are encrypted on your device before they leave."
                )
            }

            Spacer()
        }
        .padding(.horizontal, 32)
        .padding(.top, 40)
        .padding(.bottom, 120)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func featureRow(title: String, body bodyText: LocalizedStringKey) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(LascoFont.title(16))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)
            Text(bodyText)
                .font(LascoFont.body(16))
                .foregroundStyle(Color.Lasco.inkSub)
                .fixedSize(horizontal: false, vertical: true)
                .lineSpacing(4)
        }
    }

    private var existingLibraryPage: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Are you new in Lasco?")
                .font(LascoFont.title(26))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)

            Text("If you've already set up a Lasco library on another device, you can add it here.")
                .font(LascoFont.body(16))
                .foregroundStyle(Color.Lasco.inkSub)
                .fixedSize(horizontal: false, vertical: true)
                .lineSpacing(4)

            Spacer()

            VStack(spacing: 12) {
                Button("Yes, start fresh") {
                    slideForward = true
                    withAnimation(.easeInOut(duration: 0.3)) { flow = .own }
                }
                .buttonStyle(LascoPrimaryButtonStyle())
                .frame(maxWidth: .infinity)

                Button("No, I have an existing Lasco library") {
                    slideForward = true
                    withAnimation(.easeInOut(duration: 0.3)) { flow = .existingChoice }
                }
                .buttonStyle(LascoSecondaryButtonStyle())
                .frame(maxWidth: .infinity)
            }
            .padding(.bottom, 48)
        }
        .padding(.horizontal, 32)
        .padding(.top, 40)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var encryptedPage: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Fully encrypted.")
                .font(LascoFont.title(26))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)

            Text("Your photos are encrypted on your device before they ever leave it.")
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
        .overlay(alignment: .bottom) { mascotOverlay(encryptedMascot) }
    }

    private var betaPage: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Safety first !")
                .font(LascoFont.title(26))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)

            Text("Always make sure your data is correctly replicated at several places, within or without Lasco, before deleting things.")
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
        .overlay(alignment: .bottom) { mascotOverlay(betaMascot) }
    }

    @ViewBuilder
    private func mascotOverlay(_ image: Image?) -> some View {
        if let image {
            GeometryReader { geo in
                image
                    .resizable()
                    .scaledToFit()
                    .frame(width: geo.size.width * 2)
                    .frame(width: geo.size.width, alignment: .center)
                    .frame(maxHeight: .infinity, alignment: .top)
            }
            .frame(height: 500)
            .offset(y: 60)
            .clipped()
        }
    }

    // MARK: - Cloud steps

    private var cloudAccountStep: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Create your account.")
                .font(LascoFont.title(26))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)

            VStack(alignment: .leading, spacing: 16) {
                VStack(alignment: .leading, spacing: 6) {
                    FieldLabel(text: "Email")
                    TextField("", text: $cloudEmail)
                        .textFieldStyle(.plain)
                        .lascoInput()
                        .autocorrectionDisabled()
                        #if os(iOS)
                        .keyboardType(.emailAddress)
                        .textInputAutocapitalization(.never)
                        #endif
                }

                VStack(alignment: .leading, spacing: 6) {
                    FieldLabel(text: "Password")
                    SecureField("", text: $cloudPassword)
                        .textFieldStyle(.plain)
                        .lascoInput()
                }
            }

            Spacer()
        }
        .padding(.horizontal, 32)
        .padding(.top, 40)
        .padding(.bottom, 120)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var cloudPlanStep: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Choose a plan.")
                .font(LascoFont.title(26))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)

            VStack(alignment: .leading, spacing: 0) {
                planRow(name: "Lasco Cloud — 50 GB", price: "€3 / month")
                planRow(name: "Lasco Cloud — 200 GB", price: "€7 / month")
                planRow(name: "Lasco Cloud — 2 TB", price: "€15 / month")
            }
            .lascoPanel()

            Text("Subscription coming soon.")
                .font(LascoFont.body(13))
                .foregroundStyle(Color.Lasco.inkMuted)

            Spacer()
        }
        .padding(.horizontal, 32)
        .padding(.top, 40)
        .padding(.bottom, 120)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func planRow(name: String, price: String) -> some View {
        HStack {
            Text(name)
                .font(LascoFont.body())
                .foregroundStyle(Color.Lasco.inkSub)
            Spacer()
            Text(price)
                .font(LascoFont.mono())
                .foregroundStyle(Color.Lasco.inkMuted)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }

    private var cloudLibraryStep: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Name your library.")
                .font(LascoFont.title(26))
                .foregroundStyle(Color.Lasco.ink)
                .fixedSize(horizontal: false, vertical: true)

            VStack(alignment: .leading, spacing: 6) {
                FieldLabel(text: "Library name")
                TextField("My Photos", text: $cloudLibraryName)
                    .textFieldStyle(.plain)
                    .lascoInput()
                    .autocorrectionDisabled()
            }

            Spacer()
        }
        .padding(.horizontal, 32)
        .padding(.top, 40)
        .padding(.bottom, 120)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

}

#Preview {
    OnboardingView()
}
