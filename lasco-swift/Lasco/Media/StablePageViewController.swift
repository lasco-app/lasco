import SwiftUI

#if canImport(UIKit)
import UIKit

/// A virtual, position-based pager. It exposes every valid gallery position to
/// UIPageViewController while retaining hosting controllers only near the
/// selection. The caller may render a loading view for a position whose media
/// data is still being fetched.
struct StablePageViewController<Content: View>: UIViewControllerRepresentable {
    let selection: Int
    let pageCount: Int
    let onSelectionChanged: (Int) -> Void
    let content: (Int) -> Content

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    func makeUIViewController(context: Context) -> UIPageViewController {
        let controller = UIPageViewController(
            transitionStyle: .scroll,
            navigationOrientation: .horizontal,
            options: [.interPageSpacing: 0]
        )
        controller.dataSource = context.coordinator
        controller.delegate = context.coordinator
        context.coordinator.apply(parent: self, to: controller)
        return controller
    }

    func updateUIViewController(_ controller: UIPageViewController, context: Context) {
        context.coordinator.parent = self
        guard !context.coordinator.isTransitioning else {
            context.coordinator.hasPendingUpdate = true
            return
        }
        context.coordinator.apply(parent: self, to: controller)
    }

    final class Coordinator: NSObject, UIPageViewControllerDataSource, UIPageViewControllerDelegate {
        var parent: StablePageViewController
        var isTransitioning = false
        var hasPendingUpdate = false

        private var pageCount = 0
        private var transitionStartSelection: Int?
        private var controllers: [Int: UIHostingController<Content>] = [:]

        init(parent: StablePageViewController) {
            self.parent = parent
        }

        func apply(parent: StablePageViewController, to pageViewController: UIPageViewController) {
            pageCount = parent.pageCount
            guard pageCount > 0, (0..<pageCount).contains(parent.selection) else {
                if pageViewController.viewControllers?.isEmpty == false {
                    pageViewController.setViewControllers(nil, direction: .forward, animated: false)
                }
                return
            }

            trimControllers(around: parent.selection)
            for (position, controller) in controllers {
                controller.rootView = parent.content(position)
            }

            let target = controller(for: parent.selection)
            guard let visible = pageViewController.viewControllers?.first else {
                pageViewController.setViewControllers([target], direction: .forward, animated: false)
                return
            }
            guard visible !== target else { return }

            let direction: UIPageViewController.NavigationDirection
            if let visiblePosition = pagePosition(for: visible), parent.selection < visiblePosition {
                direction = .reverse
            } else {
                direction = .forward
            }
            pageViewController.setViewControllers([target], direction: direction, animated: false)
        }

        func pageViewController(
            _ pageViewController: UIPageViewController,
            viewControllerBefore viewController: UIViewController
        ) -> UIViewController? {
            guard let position = pagePosition(for: viewController), position > 0 else { return nil }
            return controller(for: position - 1)
        }

        func pageViewController(
            _ pageViewController: UIPageViewController,
            viewControllerAfter viewController: UIViewController
        ) -> UIViewController? {
            guard let position = pagePosition(for: viewController), position + 1 < pageCount else { return nil }
            return controller(for: position + 1)
        }

        func pageViewController(
            _ pageViewController: UIPageViewController,
            willTransitionTo pendingViewControllers: [UIViewController]
        ) {
            isTransitioning = true
            transitionStartSelection = parent.selection
        }

        func pageViewController(
            _ pageViewController: UIPageViewController,
            didFinishAnimating finished: Bool,
            previousViewControllers: [UIViewController],
            transitionCompleted completed: Bool
        ) {
            isTransitioning = false
            let selectionChangedDuringTransition = hasPendingUpdate
                && transitionStartSelection != parent.selection
            transitionStartSelection = nil

            if selectionChangedDuringTransition {
                hasPendingUpdate = false
                apply(parent: parent, to: pageViewController)
                return
            }

            if completed,
               let visible = pageViewController.viewControllers?.first,
               let position = pagePosition(for: visible) {
                parent.onSelectionChanged(position)
                hasPendingUpdate = false
            } else if hasPendingUpdate {
                hasPendingUpdate = false
                apply(parent: parent, to: pageViewController)
            }
        }

        private func controller(for position: Int) -> UIHostingController<Content> {
            if let controller = controllers[position] { return controller }
            let controller = UIHostingController(rootView: parent.content(position))
            controller.view.backgroundColor = .black
            controllers[position] = controller
            return controller
        }

        private func trimControllers(around position: Int) {
            let keepRange = (position - 2)...(position + 2)
            controllers = controllers.filter { keepRange.contains($0.key) }
        }

        private func pagePosition(for viewController: UIViewController) -> Int? {
            controllers.first { $0.value === viewController }?.key
        }
    }
}
#endif
