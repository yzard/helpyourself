import SwiftUI

struct TrainingPlannerView: View {
    var model: AppModel
    @State private var date = Date()
    @State private var kind = "calendar"
    @State private var entries: [JSONValue] = []
    @State private var editor: Selection?
    @State private var failure: String?
    private struct Selection: Identifiable { let id = UUID(); let existing: JSONValue?; let kind: String; let values: [String: String] }
    var body: some View {
        List {
            Section {
                Picker("View", selection: $kind) { Text("Calendar").tag("calendar"); Text("Exercises").tag("exercise"); Text("Templates").tag("workout_template") }
                if kind == "calendar" { DatePicker("Date", selection: $date, displayedComponents: .date) }
                Button("Add " + (kind == "calendar" ? "planned workout" : kind.replacingOccurrences(of: "_", with: " "))) {
                    editor = Selection(existing: nil, kind: kind == "calendar" ? "planned_workout" : kind, values: [:])
                }
                Text("Plans and templates do not add completed training or load. Record the completed session in Daily logs.").font(.caption)
                if let failure { Text(failure).foregroundStyle(.secondary) }
            }
            if entries.isEmpty { Text("No entries in this view.").foregroundStyle(.secondary) }
            ForEach(Array(entries.enumerated()), id: \.offset) { _, item in
                Section {
                    let content = item["entry"]["content"]
                    Text(content["name"].stringValue.isEmpty ? (content["title"].stringValue.isEmpty ? content["activity"].stringValue : content["title"].stringValue) : content["name"].stringValue).font(.headline)
                    Text(item["entry"]["kind"].stringValue.replacingOccurrences(of: "_", with: " ")).font(.caption)
                    if kind == "calendar" { Text(Date(timeIntervalSince1970: item["at"].numberValue ?? 0).formatted()) }
                    ForEach(EntryDraft.fields(item["entry"]["kind"].stringValue), id: \.key) { field in
                        LabeledContent(field.label, value: EntryDraft.decode(item["entry"])[field.key] ?? "")
                    }
                    if item["entry"]["kind"] == .string("planned_workout"), item["entry"]["content"]["status"] == .string("planned"), let at = item["at"].numberValue, let duration = item["entry"]["content"]["duration_minutes"].numberValue {
                        CalendarPlanButton(title: item["entry"]["content"]["title"].stringValue, start: Date(timeIntervalSince1970: at), durationMinutes: duration)
                    }
                    Button("Edit") { editor = Selection(existing: item, kind: item["entry"]["kind"].stringValue, values: [:]) }
                    if kind == "workout_template" {
                        Button("Schedule a copy") {
                            var values = EntryDraft.decode(item["entry"])
                            values["title"] = content["name"].stringValue
                            values["note"] = "Copied from template " + item["record_id"].stringValue + " revision " + String(Int(item["version"].numberValue ?? 0))
                            editor = Selection(existing: nil, kind: "planned_workout", values: values)
                        }
                    }
                    Button("Delete", role: .destructive) { Task { await remove(item) } }
                }
            }
        }.navigationTitle("Training planner")
            .task(id: kind + date.formatted(.iso8601)) { await load() }
            .refreshable { await load() }
            .sheet(item: $editor, onDismiss: { Task { await load() } }) { selection in
                NavigationStack { LogEditor(model: model, existing: selection.existing, initialKind: selection.kind, initialValues: selection.values, initialDate: date) }
            }
    }
    private func load() async {
        guard let client = model.client, let token = model.session?.token else { return }
        do {
            let response: JSONValue
            if kind == "calendar" {
                let start = Calendar.current.startOfDay(for: date)
                guard let end = Calendar.current.date(byAdding: .day, value: 1, to: start) else { return }
                response = try await model.request("wellness/entries/list", body: .object(["start_at": .number(start.timeIntervalSince1970), "end_at": .number(end.timeIntervalSince1970), "kind": .null]))
            } else { response = try await model.request("wellness/library", body: .object(["kind": .string(kind)])) }
            guard !Task.isCancelled, model.session?.token == token else { return }
            entries = response["entries"].arrayValue.filter { kind != "calendar" || ["planned_workout", "training", "training_day"].contains($0["entry"]["kind"].stringValue) }
            failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
    private func remove(_ item: JSONValue) async {
        await model.perform {
            guard let client = model.client else { return }
            _ = try await model.request("wellness/entries/delete", body: .object(["record_id": item["record_id"], "version": .number((item["version"].numberValue ?? 0) + 1), "batch_id": .string(UUID().uuidString), "kind": item["entry"]["kind"]]))
        }
        await load()
    }
}
