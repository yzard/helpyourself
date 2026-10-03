import SwiftUI

struct SettingsView: View {
    var model: AppModel
    @State private var exports: [JSONValue] = []
    @State private var share: ShareLocation?
    @State private var deleting = false
    @State private var confirmation = ""
    struct ShareLocation: Identifiable { let id = UUID(); let url: URL }
    var body: some View {
        List {
            Section("Your server") {
                Text(model.session?.server.absoluteString ?? "").textSelection(.enabled)
                Text(model.session?.username ?? "")
                Text("OCR and analysis use the providers chosen in your server's TOML configuration. This app does not route your data through an official helpyourself server.").font(.caption)
            }
            Section("Full data export") {
                Text("Includes original files, structured data and revision history. Export files contain sensitive health data.").font(.caption)
                Button("Create ZIP export") { Task { await action("exports/create", body: .object([:])); await reload() } }.disabled(model.isBusy)
                ForEach(exports, id: \.identifier) { item in
                    VStack(alignment: .leading) {
                        Text("\(item["status"].stringValue.capitalized) · \(Date(timeIntervalSince1970: item["created_at"].numberValue ?? 0).formatted())")
                        if item["status"] == .string("ready") { Button("Download and share") { Task { await download(item["export_id"].stringValue) } }.disabled(model.isBusy) }
                        Button("Delete export", role: .destructive) { Task { await action("exports/delete", body: .object(["export_id": item["export_id"]])); try? model.archive?.remove(name: "export-\(item["export_id"].stringValue).zip"); await reload() } }
                    }.buttonStyle(.borderless)
                }
            }
            Section {
                Button("Sign out and clear this device") { Task { await model.logout() } }.disabled(model.isBusy)
                Text("This also removes cached reports, pending uploads and local sync checkpoints.").font(.caption)
                Button("Delete account and all server data", role: .destructive) { deleting = true }.disabled(model.isBusy)
            }
        }.navigationTitle("Settings").task { await reload() }.refreshable { await reload() }
            .sheet(item: $share, onDismiss: { clearExportDownloads() }) { item in NavigationStack { VStack(spacing: 20) { Text("Your complete archive is ready"); ShareLink(item: item.url) { Label("Save or share ZIP", systemImage: "square.and.arrow.up") } }.padding().toolbar { Button("Done") { share = nil } } } }
            .alert("Delete your account?", isPresented: $deleting) {
                TextField("Type your username", text: $confirmation)
                Button("Cancel", role: .cancel) { confirmation = "" }
                Button("Delete", role: .destructive) { Task { await model.perform {
                    guard let client = model.client else { return }
                    _ = try await client.post("user/delete", body: .object(["confirmation": .string(confirmation)]))
                    try model.clearLocalSession()
                }; confirmation = "" } }
            } message: { Text("Type \(model.session?.username ?? "") to delete all server records and revoke every session. Stored files are then removed by the server. Your separate backups remain your responsibility.") }
    }
    private func reload() async { do { if let client = model.client { exports = try await client.post("exports/list", body: .object([:]))["exports"].arrayValue } } catch { model.errorMessage = error.localizedDescription } }
    private func action(_ path: String, body: JSONValue) async { await model.perform { if let client = model.client { _ = try await client.post(path, body: body) } } }
    private func download(_ id: String) async { await model.perform {
        guard let client = model.client, let archive = model.archive else { return }
        let url = try archive.location("export-\(id).zip")
        try await client.download("exports/\(id)/download", destination: url)
        share = ShareLocation(url: url)
    } }
    private func clearExportDownloads() { for item in exports { try? model.archive?.remove(name: "export-\(item["export_id"].stringValue).zip") } }
}
