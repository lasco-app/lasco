import SwiftUI
import AVKit
import UniformTypeIdentifiers

struct HideTabBarKey: PreferenceKey {
    static let defaultValue = false
    static func reduce(value: inout Bool, nextValue: () -> Bool) {
        value = value || nextValue()
    }
}

struct TitleAvailableWidthPreferenceKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

struct TitleIntrinsicWidthPreferenceKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

struct AAEViewerPayload: Identifiable {
    let id = UUID()
    let text: String
}

struct MediaDetailState: Hashable {
    let source: MediaDetailSource
    let startPosition: Int
}

struct MediaDetailView: View {
    let repository: LibraryRepository
    @State private var detailModel: MediaDetailModel
    @State private var pagerIndex = 0
    @State private var groupMediaIndex: Int = 0
    @State private var fullImages: [FfiMediaUuid: Image] = [:]
    @State private var thumbnails: [FfiMediaUuid: Image] = [:]
    @State private var videoPlayers: [FfiMediaUuid: AVPlayer] = [:]
    @State private var livePhotoVideoItems: [FfiMediaUuid: FfiMediaItem] = [:] // keyed by the still's mediaId
    @State private var showingLivePhotoVideo = false
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme
    @AppStorage("expertMode") private var expertMode = false

    // Panel reveal state (0 = image fills screen, 1 = info panel visible)
    @State private var panelProgress: CGFloat = 0
    @State private var panelOpen = false
    @State private var panelDragOffset: CGFloat = 0

    // Counter auto-hide
    @State private var showCounter = true
    @State private var counterTask: Task<Void, Never>? = nil

    // Rename sheet
    @State private var showingRename = false
    @State private var renameText = ""
    // Title truncation detection
    @State private var titleAvailableWidth: CGFloat = 0
    @State private var titleIntrinsicWidth: CGFloat = 0

    #if canImport(UIKit)
    @State private var showingExportSheet = false
    #endif

    // AAE adjustment data debug viewer
    @State private var aaePayload: AAEViewerPayload? = nil
    @State private var exportData: Data?
    @State private var exportURL: URL?

    var currentAlbumId: FfiAlbumUuid? { detailModel.source.currentAlbumID }
    var onAlbumTap: ((FfiAlbum) -> Void)? = nil

    init(
        source: MediaDetailSource,
        startPosition: Int,
        repository: LibraryRepository,
        onAlbumTap: ((FfiAlbum) -> Void)? = nil
    ) {
        self.repository = repository
        self._detailModel = State(initialValue: MediaDetailModel(
            source: source, startPosition: startPosition, repository: repository
        ))
        self.onAlbumTap = onAlbumTap
    }

    private var items: [AlbumItem] { detailModel.neighbors?.items ?? [] }
    private var currentIndex: Int { detailModel.neighbors?.currentIndex ?? 0 }
    private var currentPosition: Int? { detailModel.neighbors?.currentPosition }

    private var currentGroupMedia: [FfiMediaItem] {
        guard case .group(let g) = items[safe: currentIndex] else { return [] }
        return detailModel.groupMedia[g.groupId] ?? []
    }

    private var currentDisplayItem: FfiMediaItem? {
        guard items.indices.contains(currentIndex) else { return nil }
        switch items[currentIndex] {
        case .media(let m):
            return detailModel.currentMedia?.mediaId == m.mediaId ? detailModel.currentMedia : m
        case .group:
            let media = currentGroupMedia
            return media.indices.contains(groupMediaIndex) ? media[groupMediaIndex] : nil
        }
    }

    private var currentItem: FfiMediaItem? { currentDisplayItem }

    private var otherContainingAlbums: [FfiAlbum] {
        guard let mediaID = currentItem?.mediaId,
              detailModel.currentMedia?.mediaId == mediaID else { return [] }
        return detailModel.containingAlbums.filter { $0.albumId != currentAlbumId }
    }

    private var currentLivePhotoVideoItem: FfiMediaItem? {
        guard let item = currentItem else { return nil }
        return livePhotoVideoItems[item.mediaId]
    }

    // Metadata display only. Rename, export, and album membership stay on currentItem,
    // since the paired video isn't a renamable/exportable library item on its own.
    private var infoDisplayItem: FfiMediaItem? {
        showingLivePhotoVideo ? (currentLivePhotoVideoItem ?? currentItem) : currentItem
    }

    private func loadLivePhotoVideoIfNeeded(for item: FfiMediaItem) {
        guard let videoId = item.appleLivePhotoMediaId, livePhotoVideoItems[item.mediaId] == nil else { return }
        Task {
            if let video = try? await repository.showMedia(id: videoId) {
                livePhotoVideoItems[item.mediaId] = video
            }
        }
    }

    private var isTitleTruncated: Bool { titleIntrinsicWidth > titleAvailableWidth }
    private var useCompactTitle: Bool { panelOpen && isTitleTruncated }

    private func itemForMediaId(_ mediaId: FfiMediaUuid) -> FfiMediaItem? {
        for item in items {
            if case .media(let m) = item, m.mediaId == mediaId { return m }
        }
        for media in detailModel.groupMedia.values {
            if let m = media.first(where: { $0.mediaId == mediaId }) { return m }
        }
        return nil
    }

    private func loadGroupMediaIfNeeded(for groupId: FfiGroupUuid) {
        Task {
            await detailModel.loadGroupMediaIfNeeded(groupID: groupId)
        }
    }

    var body: some View {
        Group {
            #if canImport(UIKit)
            iOSBody
            #else
            macOSBody
            #endif
        }
        .sheet(isPresented: $showingRename) {
            RenameMediaSheet(
                originalFilename: currentItem?.filenameOriginal ?? "",
                name: $renameText,
                onConfirm: confirmRename
            )
            .environment(\.lascoTheme, .dark)
            .preferredColorScheme(.dark)
        }
        .sheet(item: $aaePayload) { payload in
            AAEAdjustmentDebugView(jsonText: payload.text)
        }
        .task {
            await detailModel.start()
        }
        .task(id: currentItem?.mediaId) {
            guard let mediaID = currentItem?.mediaId else { return }
            await detailModel.refreshMedia(id: mediaID)
        }
    }

    private func presentAAEAdjustment(mediaId: FfiMediaUuid) {
        Task {
            guard let data = try? await repository.mediaBytes(mediaID: mediaId) else {
                aaePayload = AAEViewerPayload(text: "no adjustment data")
                return
            }
            aaePayload = AAEViewerPayload(text: AAEDecoder.decodeAdjustmentJSON(from: data) ?? "no adjustment data")
        }
    }

    // MARK: - iOS

    #if canImport(UIKit)
    private var statusBarHeight: CGFloat {
        UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first?.windows.first?.safeAreaInsets.top ?? 44
    }

    private static let collapsedPanelHeight: CGFloat = 72
    private static let panelExpandRatio: CGFloat = 0.58

    private var iOSBody: some View {
        GeometryReader { geo in
            ZStack(alignment: .bottom) {
                Color.black.ignoresSafeArea()

                // Full-screen pager — upper area is free for L/R swipe
                TabView(selection: $pagerIndex) {
                    ForEach(Array(items.enumerated()), id: \.element.id) { idx, item in
                        resolvedMediaCell(for: item, atIndex: idx, size: geo.size)
                            .tag(idx)
                            .task(id: item.id) {
                                if case .group(let g) = item {
                                    loadGroupMediaIfNeeded(for: g.groupId)
                                }
                            }
                    }
                }
                .tabViewStyle(.page(indexDisplayMode: .never))
                .frame(width: geo.size.width, height: geo.size.height)
                .onChange(of: pagerIndex) {
                    let delta = pagerIndex - currentIndex
                    guard delta != 0 else { return }
                    detailModel.move(by: delta)
                }
                .onChange(of: groupMediaIndex) {
                    showingLivePhotoVideo = false
                }

                // Group thumbnail strip (shown when current item is a group)
                if case .group = items[safe: currentIndex], !currentGroupMedia.isEmpty {
                    GroupThumbnailStrip(media: currentGroupMedia, selected: $groupMediaIndex)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
                        .padding(.bottom, 80)
                        .allowsHitTesting(true)
                }

                // Info panel — drag handle is the only vertical-swipe zone
                infoPanelSheet(geo: geo)

                // Floating controls always on top
                floatingControls(safeTop: statusBarHeight)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
        }
        .ignoresSafeArea()
        .preferredColorScheme(.dark)
        .navigationBarBackButtonHidden(true)
        .navigationTitle("")
        .hideSystemNavigationBar()
        .toolbarBackButton(action: { dismiss() })
        .preference(key: HideTabBarKey.self, value: true)
        .onChange(of: currentPosition) {
            pagerIndex = currentIndex
            AppLogger.log(.info, "media navigated — '\(currentItem.map { $0.name ?? $0.filenameOriginal } ?? "")' (\(currentItem?.mediaId.value ?? "group"))")
            if case .group(let g) = items[safe: currentIndex] { loadGroupMediaIfNeeded(for: g.groupId) }
            preloadAdjacent()
            groupMediaIndex = 0
            showingLivePhotoVideo = false
            withAnimation { showCounter = true }
            scheduleCounterHide()
        }
    }

    private func infoPanelSheet(geo: GeometryProxy) -> some View {
        let expandedH = geo.size.height * Self.panelExpandRatio
        let travelH = expandedH - Self.collapsedPanelHeight
        let baseY: CGFloat = panelOpen ? 0 : travelH
        let currentY = max(0, min(travelH, baseY + panelDragOffset))

        return panelBody
            .frame(height: expandedH)
            .frame(width: geo.size.width)
            .offset(y: currentY)
            .gesture(
                DragGesture(minimumDistance: 10, coordinateSpace: .local)
                    .onChanged { value in
                        panelDragOffset = value.translation.height
                        let offset = max(0, min(travelH, baseY + value.translation.height))
                        panelProgress = 1 - offset / travelH
                    }
                    .onEnded { value in
                        let predicted = max(0, min(travelH, baseY + value.predictedEndTranslation.height))
                        let shouldOpen = predicted < travelH / 2
                        withAnimation(.spring(response: 0.4, dampingFraction: 0.85)) {
                            panelOpen = shouldOpen
                            panelDragOffset = 0
                            panelProgress = shouldOpen ? 1 : 0
                        }
                    }
            )
    }

    private var panelBody: some View {
        let p = LascoTheme.dark
        let progress = panelProgress
        return VStack(alignment: .leading, spacing: 0) {
            HStack {
                Spacer()
                Rectangle()
                    .fill(p.inkMuted.opacity(0.4))
                    .frame(width: 36, height: 3)
                Spacer()
            }
            .padding(.top, 8)
            .frame(height: 20)
            .frame(maxWidth: .infinity)

            HStack(alignment: useCompactTitle ? .top : .center, spacing: 8) {
                Text(currentItem.flatMap { $0.name } ?? "")
                    .font(useCompactTitle ? LascoFont.pixel(15) : LascoFont.title())
                    .lineLimit(useCompactTitle ? nil : 1)
                    .truncationMode(.tail)
                    .foregroundStyle(p.ink)
                    .background(
                        GeometryReader { geo in
                            Color.clear.preference(key: TitleAvailableWidthPreferenceKey.self, value: geo.size.width)
                        }
                    )
                    .onPreferenceChange(TitleAvailableWidthPreferenceKey.self) { titleAvailableWidth = $0 }
                    .background(
                        Text(currentItem.flatMap { $0.name } ?? "")
                            .font(LascoFont.title())
                            .fixedSize(horizontal: true, vertical: false)
                            .hidden()
                            .background(
                                GeometryReader { geo in
                                    Color.clear.preference(key: TitleIntrinsicWidthPreferenceKey.self, value: geo.size.width)
                                }
                            )
                    )
                    .onPreferenceChange(TitleIntrinsicWidthPreferenceKey.self) { titleIntrinsicWidth = $0 }
                Spacer()
                Button(action: beginRename) {
                    Image("pencil").renderingMode(.template).resizable().frame(width: 18, height: 18)
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(p.ink)
                        .padding(8)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .opacity(showingLivePhotoVideo ? progress * 0.3 : progress)
                .disabled(progress < 0.5 || showingLivePhotoVideo)
            }
            .padding(.horizontal, 20)
            .frame(minHeight: 52, alignment: useCompactTitle ? .top : .leading)
            .frame(maxWidth: .infinity, alignment: .leading)

            Rectangle()
                .fill(p.ink.opacity(0.15))
                .frame(height: 2)

            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    VStack(alignment: .leading, spacing: 8) {
                        aaeNotice(p: p)
                        metaRow(label: "FILE", value: infoDisplayItem?.filenameOriginal ?? "")
                        metaRow(label: "DATE", value: infoDisplayItem.map { formatMediaDate($0.date) } ?? "")
                        metaRow(label: "SIZE", value: formattedSize)
                        metaRow(label: "ADDED BY", value: infoDisplayItem?.author ?? "")
                        if expertMode {
                            metaRow(label: "ID", value: infoDisplayItem?.mediaId.value ?? "")
                            metaRow(label: "HASH", value: infoDisplayItem?.contentHash ?? "")
                            if let aaeMediaId = infoDisplayItem?.appleAaeMediaId {
                                Button(action: { presentAAEAdjustment(mediaId: aaeMediaId) }) {
                                    metaRow(label: "AAE", value: aaeMediaId.value)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                    .padding(20)

                    exportButton(p: p)
                        .padding(.horizontal, 20)
                        .padding(.top, 4)
                        .padding(.bottom, 12)

                    if !showingLivePhotoVideo, !otherContainingAlbums.isEmpty {
                        alsoInSection
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .opacity(progress)
            .disabled(progress < 0.5)
        }
        .background(Color.black)
        .sheet(isPresented: $showingExportSheet) {
            if let url = exportURL {
                ActivityView(activityItems: [url])
            } else if let data = exportData {
                ActivityView(activityItems: [data])
            }
        }
    }

    @ViewBuilder
    private func floatingControls(safeTop: CGFloat) -> some View {
        HStack {
            Button(action: { dismiss() }) {
                Image("angle-left").renderingMode(.template).resizable().frame(width: 18, height: 18)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Color.white)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .background(Color.black)
                    .overlay(Rectangle().stroke(Color.white, lineWidth: 2))
            }
            .buttonStyle(.plain)

            Spacer()

            HStack(spacing: 8) {
                if currentLivePhotoVideoItem != nil {
                    livePhotoToggleButton
                }

                if showCounter, !positionLabel.isEmpty {
                    Text(positionLabel)
                        .font(LascoFont.pixel())
                        .foregroundStyle(Color.white)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(Color.black.opacity(0.5))
                        .overlay(Rectangle().stroke(Color.white.opacity(0.5), lineWidth: 1))
                        .transition(.opacity)
                }
            }
        }
        .padding(.horizontal, 16)
        .padding(.top, safeTop + 8)
        .animation(.easeOut(duration: 0.2), value: showCounter)
    }

    private var livePhotoToggleButton: some View {
        Button(action: { showingLivePhotoVideo.toggle() }) {
            Image(showingLivePhotoVideo ? "image" : "play").renderingMode(.template).resizable().frame(width: 18, height: 18)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(Color.white)
                .padding(.horizontal, 10)
                .padding(.vertical, 6)
                .background(Color.black)
                .overlay(Rectangle().stroke(Color.white, lineWidth: 2))
        }
        .buttonStyle(.plain)
    }

    private func exportButton(p: LascoTheme) -> some View {
        Button {
            guard let item = currentItem else { return }
            Task {
                exportData = nil
                exportURL = nil
                if isVideo(item) {
                    exportURL = try? await repository.materializedMediaURL(
                        mediaID: item.mediaId,
                        originalFilename: item.filenameOriginal
                    )
                    showingExportSheet = exportURL != nil
                } else {
                    exportData = try? await repository.mediaBytesAsync(mediaID: item.mediaId)
                    showingExportSheet = exportData != nil
                }
            }
        } label: {
            HStack(spacing: 8) {
                Image("share").renderingMode(.template).resizable().frame(width: 16, height: 16)
                Text("EXPORT")
                    .font(LascoFont.pixel())
            }
            .foregroundStyle(p.ink)
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .overlay(Rectangle().stroke(p.ink.opacity(0.4), lineWidth: 1))
        }
        .buttonStyle(.plain)
    }

    private func scheduleCounterHide() {
        counterTask?.cancel()
        counterTask = Task {
            try? await Task.sleep(for: .seconds(1))
            guard !Task.isCancelled else { return }
            await MainActor.run {
                withAnimation(.easeOut(duration: 0.3)) { showCounter = false }
            }
        }
    }

    @ViewBuilder
    private func resolvedMediaCell(for albumItem: AlbumItem, atIndex cellIdx: Int, size: CGSize) -> some View {
        switch albumItem {
        case .media(let m):
            mediaCell(for: m, size: size)
        case .group(let g):
            let media = detailModel.groupMedia[g.groupId] ?? []
            let displayIdx = (cellIdx == currentIndex) ? groupMediaIndex : 0
            if let m = media[safe: displayIdx] {
                mediaCell(for: m, size: size)
            } else {
                Color.black
                    .frame(width: size.width, height: size.height)
            }
        }
    }

    private func mediaCell(for item: FfiMediaItem, size: CGSize) -> some View {
        let isCurrent = item.mediaId == currentItem?.mediaId
        let liveVideo = (isCurrent && showingLivePhotoVideo) ? livePhotoVideoItems[item.mediaId] : nil
        return ZStack {
            Color.black
            if isVideo(item) {
                videoCell(for: item, size: size, isActive: isCurrent)
            } else if let liveVideo {
                videoCell(for: liveVideo, size: size, isActive: isCurrent)
            } else if let full = fullImages[item.mediaId] {
                full
                    .resizable()
                    .scaledToFit()
                    .frame(width: size.width, height: size.height)
            } else if let thumb = thumbnails[item.mediaId] {
                thumb
                    .resizable()
                    .scaledToFit()
                    .frame(width: size.width, height: size.height)
                    .blur(radius: 4)
            } else {
                Image("image").renderingMode(.template).resizable().frame(width: 18, height: 18)
                    .font(.system(size: 72))
                    .foregroundStyle(Color.white.opacity(0.3))
            }
        }
        .frame(width: size.width, height: size.height)
        .task(id: item.mediaId) {
            await loadImagesAsync(for: item.mediaId)
            loadLivePhotoVideoIfNeeded(for: item)
        }
    }
    #endif

    // MARK: - macOS

    #if !canImport(UIKit)
    private static let wideThreshold: CGFloat = 700
    private static let infoPanelWidth: CGFloat = 340
    private static let narrowBarHeight: CGFloat = 60

    private var macOSBody: some View {
        GeometryReader { geo in
            if geo.size.width >= Self.wideThreshold {
                macOSWideLayout(geo: geo)
            } else {
                macOSNarrowLayout(geo: geo)
            }
        }
        .background(Color.black)
        .navigationBarBackButtonHidden(true)
        .navigationTitle("")
        .hideSystemNavigationBar()
        .toolbarBackButton(action: { dismiss() })
        .preference(key: HideTabBarKey.self, value: true)
        .onChange(of: currentPosition) {
            if case .group(let g) = items[safe: currentIndex] { loadGroupMediaIfNeeded(for: g.groupId) }
            showingLivePhotoVideo = false
            preloadAdjacent()
        }
    }

    // Wide: media fills left side, info panel fixed on right
    private func macOSWideLayout(geo: GeometryProxy) -> some View {
        let mediaWidth = geo.size.width - Self.infoPanelWidth
        return HStack(spacing: 0) {
            VStack(spacing: 0) {
                macOSMediaCell(for: currentItem, size: CGSize(width: mediaWidth, height: geo.size.height - Self.narrowBarHeight))
                macOSNavBar
            }
            .frame(width: mediaWidth)

            infoSection
                .frame(width: Self.infoPanelWidth)
                .frame(maxHeight: .infinity, alignment: .top)
                .animation(.easeInOut(duration: 0.15), value: currentPosition)
        }
    }

    // Narrow: media fills the whole view, panel anchored to bottom
    // Collapsed: only the bar (nav + toggle) is visible
    // Expanded: bar + info content slides up over the media
    private func macOSNarrowLayout(geo: GeometryProxy) -> some View {
        let panelExpandedHeight = geo.size.height * 0.6
        let collapsedOffset = panelExpandedHeight - Self.narrowBarHeight

        return ZStack(alignment: .bottom) {
            Color.black

            macOSMediaCell(for: currentItem, size: CGSize(
                width: geo.size.width, height: geo.size.height
            ))

            // Panel: bar on top, info content below — slides as one unit
            VStack(spacing: 0) {
                macOSNarrowBar
                    .frame(height: Self.narrowBarHeight)
                infoSection
                    .frame(maxWidth: .infinity)
                Color.black
            }
            .frame(width: geo.size.width, height: panelExpandedHeight)
            .offset(y: panelOpen ? 0 : collapsedOffset)
            .animation(.spring(response: 0.35, dampingFraction: 0.85), value: panelOpen)
        }
    }

    private func macOSMediaCell(for item: FfiMediaItem?, size: CGSize) -> some View {
        let liveVideo = showingLivePhotoVideo ? item.flatMap { livePhotoVideoItems[$0.mediaId] } : nil
        return ZStack {
            Color.black
            if let item {
                if isVideo(item) {
                    videoCell(for: item, size: size, isActive: true)
                } else if let liveVideo {
                    videoCell(for: liveVideo, size: size, isActive: true)
                } else if let full = fullImages[item.mediaId] {
                    full.resizable().scaledToFit().frame(width: size.width, height: size.height)
                } else if let thumb = thumbnails[item.mediaId] {
                    thumb.resizable().scaledToFit().frame(width: size.width, height: size.height).blur(radius: 4)
                } else {
                    Image("image").renderingMode(.template).resizable().frame(width: 72, height: 72).foregroundStyle(Color.white.opacity(0.3))
                }
            }
        }
        .frame(width: size.width, height: size.height)
        .task(id: item?.mediaId) {
            guard let item else { return }
            loadLivePhotoVideoIfNeeded(for: item)
            if isVideo(item) {
                loadVideoPlayerIfNeeded(for: item)
                if item.mediaId == currentItem?.mediaId {
                    videoPlayers[item.mediaId]?.play()
                }
            } else {
                await loadImagesAsync(for: item.mediaId)
            }
        }
    }

    // Shared nav bar for wide layout
    private var macOSNavBar: some View {
        HStack(spacing: 12) {
            navPrevButton
            Text(positionLabel)
                .font(LascoFont.pixel())
                .foregroundStyle(Color.white.opacity(0.6))
            navNextButton
            if currentLivePhotoVideoItem != nil {
                livePhotoToggleButton
            }
        }
        .padding(.horizontal, 20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .frame(height: Self.narrowBarHeight)
        .background(Color.black)
        .overlay(alignment: .top) {
            Rectangle().fill(Color.white.opacity(0.15)).frame(height: 1)
        }
    }

    // Bottom bar for narrow layout: prev/next/counter + up-arrow toggle
    private var macOSNarrowBar: some View {
        HStack(spacing: 12) {
            navPrevButton
            Text(positionLabel)
                .font(LascoFont.pixel())
                .foregroundStyle(Color.white.opacity(0.6))
            navNextButton
            if currentLivePhotoVideoItem != nil {
                livePhotoToggleButton
            }
            Spacer()
            Button {
                withAnimation(.spring(response: 0.35, dampingFraction: 0.85)) {
                    panelOpen.toggle()
                }
            } label: {
                Image(panelOpen ? "chevron-down" : "chevron-up")
                    .renderingMode(.template)
                    .resizable()
                    .frame(width: 14, height: 14)
                    .foregroundStyle(Color.white)
                    .frame(width: 36, height: 36)
                    .contentShape(Rectangle())
                    .overlay(Rectangle().stroke(Color.white, lineWidth: 2))
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity)
        .background(Color.black)
    }

    private var navPrevButton: some View {
        Button {
            detailModel.move(by: -1)
        } label: {
            Image("angle-left").renderingMode(.template).resizable().frame(width: 18, height: 18)
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(detailModel.neighbors?.previous != nil ? Color.white : Color.white.opacity(0.3))
                .frame(width: 36, height: 36)
                .contentShape(Rectangle())
                .overlay(Rectangle().stroke(Color.white, lineWidth: 2))
        }
        .buttonStyle(.plain)
        .disabled(detailModel.neighbors?.previous == nil)
    }

    private var livePhotoToggleButton: some View {
        Button(action: { showingLivePhotoVideo.toggle() }) {
            Image(showingLivePhotoVideo ? "image" : "play").renderingMode(.template).resizable().frame(width: 16, height: 16)
                .foregroundStyle(Color.white)
                .frame(width: 36, height: 36)
                .contentShape(Rectangle())
                .overlay(Rectangle().stroke(Color.white, lineWidth: 2))
        }
        .buttonStyle(.plain)
    }

    private var navNextButton: some View {
        Button {
            detailModel.move(by: 1)
        } label: {
            Image("angle-right").renderingMode(.template).resizable().frame(width: 18, height: 18)
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(detailModel.neighbors?.next != nil ? Color.white : Color.white.opacity(0.3))
                .frame(width: 36, height: 36)
                .contentShape(Rectangle())
                .overlay(Rectangle().stroke(Color.white, lineWidth: 2))
        }
        .buttonStyle(.plain)
        .disabled(detailModel.neighbors?.next == nil)
    }
    #endif

    // MARK: - Shared

    private func isVideo(_ item: FfiMediaItem) -> Bool {
        let ext = (item.filenameOriginal as NSString).pathExtension.lowercased()
        return UTType(filenameExtension: ext)?.conforms(to: .audiovisualContent) == true
    }

    @ViewBuilder
    private func videoCell(for item: FfiMediaItem, size: CGSize, isActive: Bool) -> some View {
        let player = videoPlayer(for: item)
        VideoPlayer(player: player)
            .frame(width: size.width, height: size.height)
            .task(id: item.mediaId) {
                loadVideoPlayerIfNeeded(for: item)
                if isActive {
                    videoPlayers[item.mediaId]?.play()
                }
            }
            .onChange(of: isActive) { active in
                if active {
                    player?.play()
                } else {
                    player?.pause()
                }
            }
    }

    private func videoPlayer(for item: FfiMediaItem) -> AVPlayer? {
        videoPlayers[item.mediaId]
    }

    private func loadVideoPlayerIfNeeded(for item: FfiMediaItem) {
        guard videoPlayers[item.mediaId] == nil else { return }
        Task {
            guard let url = try? await repository.materializedMediaURL(
                mediaID: item.mediaId,
                originalFilename: item.filenameOriginal
            ) else { return }
            videoPlayers[item.mediaId] = AVPlayer(url: url)
        }
    }

    private func loadImagesAsync(for mediaId: FfiMediaUuid) async {
        guard let item = itemForMediaId(mediaId), !isVideo(item) else { return }
        if thumbnails[mediaId] == nil,
           let data = try? await repository.thumbnailAsync(mediaID: mediaId),
           let img = Image(data: data) {
            thumbnails[mediaId] = img
        }
        if fullImages[mediaId] == nil,
           let data = try? await repository.nativeMediaBytesAsync(mediaID: mediaId),
           let img = Image(data: data) {
            fullImages[mediaId] = img
        }
    }

    private func preloadAdjacent() {
        let idx = currentIndex
        guard items.indices.contains(idx) else { return }
        preloadFirstFrame(of: items[idx])
        for neighborIdx in [idx - 1, idx + 1] where items.indices.contains(neighborIdx) {
            preloadFirstFrame(of: items[neighborIdx])
        }
        evictDistantFullImages(around: idx)
    }

    private func preloadFirstFrame(of item: AlbumItem) {
        switch item {
        case .media(let m):
            Task { await loadImagesAsync(for: m.mediaId) }
        case .group(let g):
            loadGroupMediaIfNeeded(for: g.groupId)
            if let first = detailModel.groupMedia[g.groupId]?.first {
                Task { await loadImagesAsync(for: first.mediaId) }
            }
        }
    }

    private func evictDistantFullImages(around idx: Int, window: Int = 2) {
        let keepRange = (idx - window)...(idx + window)
        let keepIds = Set(items.indices.flatMap { i -> [FfiMediaUuid] in
            guard keepRange.contains(i) else { return [] }
            switch items[i] {
            case .media(let media): return [media.mediaId]
            case .group(let group): return detailModel.groupMedia[group.groupId]?.map(\.mediaId) ?? []
            }
        })
        for mediaId in fullImages.keys where !keepIds.contains(mediaId) {
            fullImages.removeValue(forKey: mediaId)
        }
    }

    private var positionLabel: String {
        guard let currentPosition, let totalCount = detailModel.totalCount else { return "" }
        return "\(currentPosition + 1) / \(totalCount)"
    }

    // MARK: - Info section

    private var infoSection: some View {
        let p = LascoTheme.dark
        return VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 16) {
                HStack(spacing: 8) {
                    Text(currentItem.flatMap { $0.name } ?? "")
                        .font(LascoFont.title())
                        .foregroundStyle(p.ink)
                    Spacer()
                    Button(action: beginRename) {
                        Image("pencil").renderingMode(.template).resizable().frame(width: 18, height: 18)
                            .font(.system(size: 14, weight: .semibold))
                            .foregroundStyle(p.ink)
                            .padding(8)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .opacity(showingLivePhotoVideo ? 0.3 : 1)
                    .disabled(showingLivePhotoVideo)
                }

                VStack(alignment: .leading, spacing: 8) {
                    aaeNotice(p: p)
                    metaRow(label: "FILE", value: infoDisplayItem?.filenameOriginal ?? "")
                    metaRow(label: "DATE", value: infoDisplayItem.map { formatMediaDate($0.date) } ?? "")
                    metaRow(label: "SIZE", value: formattedSize)
                    if expertMode {
                        metaRow(label: "ID", value: infoDisplayItem?.mediaId.value ?? "")
                        if let aaeMediaId = infoDisplayItem?.appleAaeMediaId {
                            Button(action: { presentAAEAdjustment(mediaId: aaeMediaId) }) {
                                metaRow(label: "AAE", value: aaeMediaId.value)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
            }
            .padding(20)

            if !showingLivePhotoVideo, !otherContainingAlbums.isEmpty {
                alsoInSection
            }
        }
        .background(p.bg)
    }

    @ViewBuilder
    private func aaeNotice(p: LascoTheme) -> some View {
        if infoDisplayItem?.appleAaeMediaId != nil {
            HStack(alignment: .top, spacing: 8) {
                Image("info-circle").renderingMode(.template).resizable().frame(width: 16, height: 16)
                    .foregroundStyle(p.inkMuted)
                Text("There is an associated metadata edit file (.aae). No worries, it's included in the library. However, it's not currently used when showing the photo (crop, rotation, etc).")
                    .font(LascoFont.mono())
                    .foregroundStyle(p.inkMuted)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.bottom, 8)
        }
    }

    private func metaRow(label: String, value: String) -> some View {
        let p = LascoTheme.dark
        return HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text(label)
                .font(LascoFont.pixel())
                .foregroundStyle(p.inkMuted)
                .frame(width: 44, alignment: .leading)
            Text(value)
                .font(LascoFont.mono())
                .foregroundStyle(p.ink)
                .textSelection(.enabled)
        }
    }

    private var alsoInSection: some View {
        let p = LascoTheme.dark
        return VStack(alignment: .leading, spacing: 12) {
            Rectangle()
                .fill(p.ink.opacity(0.15))
                .frame(height: 1)
            Text(currentAlbumId == nil
                ? (otherContainingAlbums.count == 1 ? "CONTAINED IN THIS ALBUM" : "CONTAINED IN THESE ALBUMS")
                : "ALSO IN THESE ALBUMS")
                .font(LascoFont.pixel())
                .foregroundStyle(p.inkMuted)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 120), spacing: 8)], spacing: 8) {
                ForEach(otherContainingAlbums, id: \.albumId) { album in
                    AlbumCell(album: album)
                        .onTapGesture { onAlbumTap?(album) }
                }
            }
        }
        .padding(.horizontal, 20)
        .padding(.bottom, 20)
    }

    private func beginRename() {
        renameText = currentItem?.name ?? ""
        showingRename = true
    }

    private func confirmRename() {
        let trimmed = renameText.trimmingCharacters(in: .whitespaces)
        let newName: String? = trimmed.isEmpty ? nil : trimmed
        guard let mediaId = currentItem?.mediaId else { return }
        Task {
            try? await repository.renameMedia(id: mediaId, name: newName)
        }
        showingRename = false
    }

    private var formattedSize: String {
        guard let bytes = infoDisplayItem?.sizeBytes else { return "" }
        let mb = Double(bytes) / 1_048_576
        if mb < 1 { return String(format: "%.0f KB", Double(bytes) / 1024) }
        return String(format: "%.1f MB", mb)
    }
}

#if canImport(UIKit)
private struct ActivityView: UIViewControllerRepresentable {
    let activityItems: [Any]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: activityItems, applicationActivities: nil)
    }

    func updateUIViewController(_ uiViewController: UIActivityViewController, context: Context) {}
}
#endif

private struct RenameMediaSheet: View {
    let originalFilename: String
    @Binding var name: String
    var onConfirm: () -> Void
    @Environment(\.dismiss) private var dismiss
    @Environment(\.lascoTheme) var theme
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("RENAME")
                .font(LascoFont.categoryLarge())
                .foregroundStyle(theme.ink)

            TextField(originalFilename, text: $name)
                .font(LascoFont.body())
                .textFieldStyle(.plain)
                .padding(12)
                .lascoPanel()
                .focused($focused)
                .onSubmit { onConfirm() }

            Text("Leave empty to clear the name and fall back to the original filename.")
                .font(LascoFont.pixel())
                .foregroundStyle(theme.inkMuted)

            HStack(spacing: 12) {
                Button("Cancel") { dismiss() }
                    .buttonStyle(.plain)
                    .font(LascoFont.body())
                    .foregroundStyle(theme.inkMuted)

                Spacer()

                Button("Confirm") { onConfirm() }
                    .buttonStyle(.plain)
                    .font(LascoFont.body())
                    .foregroundStyle(theme.ink)
            }
        }
        .padding(24)
        .background(theme.bg)
        .presentationDetents([.height(260)])
        .onAppear { focused = true }
    }
}

// MARK: - GroupThumbnailStrip

#if canImport(UIKit)
struct GroupThumbnailStrip: View {
    @Environment(LibraryRepository.self) private var repository
    let media: [FfiMediaItem]
    @Binding var selected: Int

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    ForEach(Array(media.enumerated()), id: \.element.mediaId) { idx, item in
                        ThumbnailCell(item: item, isSelected: idx == selected)
                            .id(idx)
                            .onTapGesture { selected = idx }
                    }
                }
                .padding(.horizontal, 12)
            }
            .frame(height: 66)
            .background(Color.black.opacity(0.6))
            .onChange(of: selected) { idx in
                withAnimation { proxy.scrollTo(idx, anchor: .center) }
            }
        }
    }

    private struct ThumbnailCell: View {
        @Environment(LibraryRepository.self) private var repository
        let item: FfiMediaItem
        let isSelected: Bool
        @State private var thumbnail: Image? = nil

        var body: some View {
            ZStack {
                Color.gray.opacity(0.3)
                if let thumbnail {
                    thumbnail.resizable().scaledToFill()
                }
            }
            .frame(width: 52, height: 52)
            .clipShape(RoundedRectangle(cornerRadius: 4))
            .overlay(
                RoundedRectangle(cornerRadius: 4)
                    .stroke(isSelected ? Color(red: 1, green: 0.2, blue: 0.6) : Color.clear, lineWidth: 2)
            )
            .task(id: item.mediaId) {
                if let data = try? await repository.thumbnailAsync(mediaID: item.mediaId) {
                    thumbnail = Image(data: data)
                }
            }
        }
    }
}
#endif

// MARK: - Safe subscript

private extension Collection {
    subscript(safe index: Index) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
