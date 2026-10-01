import SwiftUI
import PDFKit
import VisionKit

struct DocumentLocation: Identifiable {
    let id = UUID()
    let url: URL
    let page: Int
}

struct DocumentPreview: View {
    let location: DocumentLocation
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        NavigationStack {
            Group {
                if PDFDocument(url: location.url) != nil { PDFPreview(url: location.url, page: location.page) }
                else if let image = UIImage(contentsOfFile: location.url.path) {
                    ScrollView([.horizontal, .vertical]) { Image(uiImage: image).resizable().scaledToFit().frame(maxWidth: 900).padding() }
                } else { ContentUnavailableView("Preview unavailable", systemImage: "doc.questionmark") }
            }
            .navigationTitle("Original · page \(location.page)")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
        }
    }
}

private struct PDFPreview: UIViewRepresentable {
    let url: URL
    let page: Int
    func makeUIView(context: Context) -> PDFView {
        let view = PDFView(); view.autoScales = true; view.displayMode = .singlePageContinuous
        view.document = PDFDocument(url: url)
        if let target = view.document?.page(at: max(0, page - 1)) { view.go(to: target) }
        return view
    }
    func updateUIView(_ view: PDFView, context: Context) {}
}

struct DocumentScanner: UIViewControllerRepresentable {
    let completion: (Result<[Data], any Error>) -> Void
    func makeCoordinator() -> Coordinator { Coordinator(completion: completion) }
    func makeUIViewController(context: Context) -> VNDocumentCameraViewController {
        let controller = VNDocumentCameraViewController(); controller.delegate = context.coordinator; return controller
    }
    func updateUIViewController(_ controller: VNDocumentCameraViewController, context: Context) {}
    @MainActor final class Coordinator: NSObject, VNDocumentCameraViewControllerDelegate {
        let completion: (Result<[Data], any Error>) -> Void
        init(completion: @escaping (Result<[Data], any Error>) -> Void) { self.completion = completion }
        func documentCameraViewController(_ controller: VNDocumentCameraViewController, didFinishWith scan: VNDocumentCameraScan) {
            do {
                let images = try (0..<scan.pageCount).map { index in
                    guard let bytes = scan.imageOfPage(at: index).pngData() else { throw APIError.invalidResponse }
                    return bytes
                }
                controller.dismiss(animated: true); completion(.success(images))
            } catch { controller.dismiss(animated: true); completion(.failure(error)) }

        }
        func documentCameraViewControllerDidCancel(_ controller: VNDocumentCameraViewController) { controller.dismiss(animated: true) }
        func documentCameraViewController(_ controller: VNDocumentCameraViewController, didFailWithError error: any Error) {
            controller.dismiss(animated: true); completion(.failure(error))
        }
    }
}
