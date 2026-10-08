import SwiftUI
import UniformTypeIdentifiers

struct TrainingImportsView: View {
    var model: AppModel
    @State private var imports: [JSONValue] = []
    @State private var cursor: JSONValue = .null
    @State private var importing = false
    @State private var busy = false
    @State private var deleting: JSONValue?
    @State private var failure: String?
    var body: some View {
        List {
            Section {
                Button("Import a training file") { importing = true }.disabled(busy)
                Text("GPX 1.1, one TCX 2 activity, or one FIT session, up to 4 MiB. Your archive retains the complete original file.").font(.caption)
                if busy { ProgressView() }
                if let failure { Text(failure).foregroundStyle(.secondary) }
            }
            Section("Imported activities") {
                if imports.isEmpty && !busy { Text("No imported activities.") }
                ForEach(Array(imports.enumerated()), id: \.offset) { _, item in
                    VStack(alignment: .leading, spacing: 6) {
                        NavigationLink(item["filename"].stringValue) { TrainingDetailView(model: model, reference: item) }.font(.headline)
                        if let at = item["start_at"].numberValue { Text(Date(timeIntervalSince1970: at).formatted()).font(.caption) }
                        if let distance = item["distance_m"].numberValue { Text("\((distance / 1000).formatted()) km") } else { Text("Distance unknown") }
                        Text(item["source_id"].stringValue).font(.caption)
                        Button("Delete imported activity", role: .destructive) { deleting = item }.disabled(busy)
                    }
                }
                if cursor != .null { Button("Load more activities") { Task { await load(reset: false) } }.disabled(busy) }
            }
            Section {
                NavigationLink("View original archive records", destination: HealthView(model: model))
                NavigationLink("Export originals and revisions", destination: SettingsView(model: model))
                Text("GPX distance uses coordinates within segments. TCX distance uses source lap totals. Elapsed duration can include pauses.").font(.caption)
            }
        }.navigationTitle("Training imports")
            .task { await load(reset: true) }.refreshable { await load(reset: true) }
            .fileImporter(isPresented: $importing, allowedContentTypes: [.data], allowsMultipleSelection: false) { result in
                switch result {
                case .success(let urls): if let url = urls.first { Task { await importFile(url) } }
                case .failure(let error): failure = error.localizedDescription
                }
            }
            .confirmationDialog("Delete this activity and its original archive? Separate exports and backups remain unchanged.", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }), titleVisibility: .visible) {
                Button("Delete imported activity", role: .destructive) { if let item = deleting { Task { await remove(item) } } }
            }
    }
    private func load(reset: Bool) async {
        guard !busy, let client = model.client, let token = model.session?.token else { return }
        busy = true
        defer { busy = false }
        do {
            let result = try await model.request("wellness/import/list", body: .object(["after_id": reset ? .null : cursor]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            imports = reset ? result["imports"].arrayValue : imports + result["imports"].arrayValue
            cursor = result["next_after_id"]; failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
    private func importFile(_ url: URL) async {
        guard !busy, let client = model.client, let token = model.session?.token else { return }
        busy = true
        do {
            let file = try await Self.readTrack(url)
            guard !Task.isCancelled, model.session?.token == token else { busy = false; return }
            _ = try await model.request("wellness/import/\(file.format)", body: .object(["filename": .string(file.filename), file.payloadField: .string(file.contents)]))
            guard !Task.isCancelled, model.session?.token == token else { busy = false; return }
            busy = false
            await load(reset: true)
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription }; busy = false }
    }
    private func remove(_ item: JSONValue) async {
        guard !busy, let client = model.client, let token = model.session?.token else { return }
        busy = true
        do {
            _ = try await model.request("wellness/import/delete", body: .object(["record_id": item["record_id"], "source_id": item["source_id"], "expected_version": item["version"]]))
            guard !Task.isCancelled, model.session?.token == token else { busy = false; return }
            busy = false
            await load(reset: true)
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription }; busy = false }
    }
    @concurrent private static func readTrack(_ url: URL) async throws -> TrainingFile {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        let format = url.pathExtension.lowercased()
        guard ["gpx", "tcx", "fit"].contains(format) else { throw TrainingFileError.format }
        let file = try FileHandle(forReadingFrom: url)
        defer { try? file.close() }
        let limit = 4 * 1024 * 1024
        var data = Data()
        while data.count <= limit {
            try Task.checkCancellation()
            guard let block = try file.read(upToCount: min(512 * 1024, limit + 1 - data.count)), !block.isEmpty else { break }
            data.append(block)
        }
        return try TrainingFile(filename: url.lastPathComponent, data: data)
    }
}
