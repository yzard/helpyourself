import SwiftUI
import UserNotifications

struct RemindersView: View {
    var model: AppModel
    @State private var reminders: [JSONValue] = []
    @State private var occurrences: [JSONValue] = []
    @State private var editor: Selection?
    @State private var status = "Device notifications are not synchronized."
    @State private var busy = false
    @State private var loaded = false
    private struct Selection: Identifiable { let id = UUID(); let item: JSONValue? }
    var body: some View {
        List {
            Section {
                Button("Add reminder") { editor = Selection(item: nil) }
                Button("Synchronize device notifications") { Task { await schedule() } }.disabled(busy)
                Text(status).font(.caption)
                Text("This device schedules up to 50 upcoming reminders within 31 days. Open this page to refresh after server edits or before the horizon expires.").font(.caption)
                Text("Quiet hours suppress reminders. A missing daylight-saving time is skipped; a repeated time uses its first occurrence.").font(.caption)
            }
            ForEach(Array(reminders.enumerated()), id: \.offset) { _, item in
                Section(item["entry"]["content"]["title"].stringValue) {
                    Text(item["entry"]["content"]["local_time"].stringValue + " · " + item["timezone"].stringValue)
                    Text(item["entry"]["content"]["enabled"].boolValue ? "Enabled" : "Disabled")
                    Button("Edit") { editor = Selection(item: item) }
                    Button("Delete", role: .destructive) { Task { await remove(item) } }.disabled(busy)
                }
            }
            Section("Upcoming occurrences") {
                ForEach(Array(occurrences.prefix(100).enumerated()), id: \.offset) { _, item in Text(item["date"].stringValue + " " + item["local_time"].stringValue + " · " + item["title"].stringValue + " · " + item["state"].stringValue) }
            }
        }.navigationTitle("Reminders").task { await load() }.refreshable { await load() }
            .sheet(item: $editor, onDismiss: { Task { await load(); await clearScheduled() } }) { selection in
                NavigationStack { LogEditor(model: model, existing: selection.item, initialKind: "reminder", initialValues: [:], initialDate: Date()) }
            }
    }
    private func load() async {
        loaded = false
        guard let client = model.client, let token = model.session?.token else { return }
        do {
            let now = floor(Date().timeIntervalSince1970)
            let library = try await model.request("wellness/library", body: .object(["kind": .string("reminder")]))
            let feed = try await model.request("wellness/reminders", body: .object(["start_at": .number(now), "end_at": .number(now + 31 * 86400)]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            reminders = library["entries"].arrayValue; occurrences = feed["occurrences"].arrayValue; loaded = true
        } catch { if !Task.isCancelled, model.session?.token == token { status = error.localizedDescription } }
    }
    private func clearScheduled() async {
        guard let token = model.session?.token else { return }
        let center = UNUserNotificationCenter.current()
        let pending = await center.pendingNotificationRequests()
        guard model.session?.token == token else { return }
        center.removePendingNotificationRequests(withIdentifiers: pending.filter { $0.identifier.hasPrefix("helpyourself-reminder:") }.map(\.identifier))
        status = "Local schedules cleared. Synchronize to apply the current server rules."
    }
    private func schedule() async {
        guard let token = model.session?.token, let user = model.session?.userID else { return }
        busy = true; defer { busy = false }
        do {
            let center = UNUserNotificationCenter.current()
            let granted = try await center.requestAuthorization(options: [.alert, .sound])
            guard model.session?.token == token else { return }
            guard granted else { status = "Notification permission is off. Records remain available."; return }
            await load(); guard loaded, model.session?.token == token else { return }
            await clearScheduled(); guard model.session?.token == token else { return }
            let upcoming = occurrences.filter { $0["state"] == .string("scheduled") && ($0["at"].numberValue ?? 0) > Date().timeIntervalSince1970 }.sorted { ($0["at"].numberValue ?? 0) < ($1["at"].numberValue ?? 0) }
            var count = 0; var last: Double?
            for item in upcoming.prefix(50) {
                guard model.session?.token == token else { return }
                let at = item["at"].numberValue ?? 0
                let id = "helpyourself-reminder:\(user):\(item["record_id"].stringValue):\(Int(at))"
                let content = UNMutableNotificationContent(); content.title = item["title"].stringValue; content.body = item["body"].stringValue; content.sound = .default
                let trigger = UNTimeIntervalNotificationTrigger(timeInterval: max(1, at - Date().timeIntervalSince1970), repeats: false)
                try await center.add(UNNotificationRequest(identifier: id, content: content, trigger: trigger))
                guard model.session?.token == token else { center.removePendingNotificationRequests(withIdentifiers: [id]); return }
                count += 1; last = at
            }
            status = "Scheduled \(count) reminders" + (last.map { " through " + Date(timeIntervalSince1970: $0).formatted() } ?? "") + ". Delivery depends on device notification settings."
        } catch { if model.session?.token == token { status = error.localizedDescription } }
    }
    private func remove(_ item: JSONValue) async {
        await model.perform {
            guard let client = model.client else { return }
            _ = try await model.request("wellness/entries/delete", body: .object(["record_id": item["record_id"], "version": .number((item["version"].numberValue ?? 0) + 1), "batch_id": .string(UUID().uuidString), "kind": .string("reminder")]))
        }
        await clearScheduled(); await load()
    }
}
