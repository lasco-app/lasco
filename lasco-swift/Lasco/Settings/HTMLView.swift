import SwiftUI
import WebKit

// License reports are generated independently of the native app. Keep their
// appearance dark even when a report has its own light stylesheet.
private let licenseAppearanceScript = """
    const style = document.createElement('style');
    style.textContent = `
      :root { color-scheme: dark !important; }
      html, body { background: #111315 !important; color: #f1f0eb !important; font-family: sans-serif !important; }
      a { color: #f4b8d5 !important; }
      pre, blockquote, .license-text { background: #191c1f !important; color: #b2b8b9 !important; border-radius: 16px; }
    `;
    document.head.appendChild(style);
    """

@MainActor
private func makeLicenseWebView() -> WKWebView {
    let configuration = WKWebViewConfiguration()
    configuration.userContentController.addUserScript(WKUserScript(
        source: licenseAppearanceScript, injectionTime: .atDocumentEnd, forMainFrameOnly: true
    ))
    let webView = WKWebView(frame: .zero, configuration: configuration)
    #if canImport(UIKit)
    webView.isOpaque = false
    webView.backgroundColor = UIColor(Color.Lasco.bg)
    #endif
    return webView
}

#if canImport(UIKit)
struct HTMLView: UIViewRepresentable {
    let fileURL: URL

    func makeUIView(context: Context) -> WKWebView {
        makeLicenseWebView()
    }

    func updateUIView(_ webView: WKWebView, context: Context) {
        webView.loadFileURL(fileURL, allowingReadAccessTo: fileURL.deletingLastPathComponent())
    }
}
#else
struct HTMLView: NSViewRepresentable {
    let fileURL: URL

    func makeNSView(context: Context) -> WKWebView {
        makeLicenseWebView()
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        webView.loadFileURL(fileURL, allowingReadAccessTo: fileURL.deletingLastPathComponent())
    }
}
#endif
