import SwiftUI

struct DailyLogsView: View {
    var model: AppModel
    @State private var entries: [JSONValue] = []
    @State private var editing: LogSelection?
    @State private var deleting: JSONValue?
    private struct LogSelection: Identifiable { let id = UUID(); let entry: JSONValue? }
    var body: some View {
        List {
            Section {
                Button("Record an observation") { editing = LogSelection(entry: nil) }
                Text("Missing fields remain unknown. Your full export includes entries and revisions.").font(.caption).foregroundStyle(.secondary)
            }
            Section("Last 30 days") {
                if entries.isEmpty { Text("No entries in this range.").foregroundStyle(.secondary) }
                ForEach(Array(entries.enumerated()), id: \.offset) { _, item in
                    VStack(alignment: .leading, spacing: 8) {
                        Text(item["entry"]["kind"].stringValue.replacingOccurrences(of: "_", with: " ").capitalized).font(.headline)
                        Text(Date(timeIntervalSince1970: item["at"].numberValue ?? 0).formatted()).font(.caption)
                        if let load = item["calculation"]["session_load_au"].numberValue { Text("Session load: \(load.formatted()) AU") }
                        DisclosureGroup("Observation") {
                            ForEach(EntryDraft.fields(item["entry"]["kind"].stringValue), id: \.key) { field in
                                LabeledContent(field.label, value: EntryDraft.decode(item["entry"])[field.key].flatMap { $0.isEmpty ? nil : $0 } ?? "Not recorded")
                            }
                        }
                        if item["entry"]["kind"] == .string("nutrition") {
                            NavigationLink("Record this meal again") { LogEditor(model: model, existing: nil, initialKind: "nutrition", initialValues: EntryDraft.decode(item["entry"]), initialDate: Date()) }
                        }
                        HStack {
                            if item["entry"]["kind"] == .string("sleep_correction") {
                                NavigationLink("Edit sleep confirmation", destination: SleepSessionsView(model: model))
                            } else { Button("Edit") { editing = LogSelection(entry: item) } }
                            Spacer()
                            Button("Delete", role: .destructive) { deleting = item }
                        }.buttonStyle(.borderless)
                    }
                }
            }
        }.navigationTitle("Daily logs")
            .task { await reload() }.refreshable { await reload() }
            .sheet(item: $editing, onDismiss: { Task { await reload() } }) { selection in
                NavigationStack { LogEditor(model: model, existing: selection.entry, initialKind: "journal", initialValues: [:], initialDate: Date()) }
            }
            .confirmationDialog("Delete this log and its revision history? Separate exports and backups remain unchanged.", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }), titleVisibility: .visible) {
                Button("Delete log", role: .destructive) { if let entry = deleting { Task { await remove(entry) } } }
            }
    }
    private func reload() async {
        guard let client = model.client, let token = model.session?.token else { return }
        do {
            let now = Date().timeIntervalSince1970
            let result = try await model.request("wellness/entries/list", body: .object(["start_at": .number(floor(now - 30 * 86400)), "end_at": .number(floor(now + 86400)), "kind": .null]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            entries = result["entries"].arrayValue
        } catch { if !Task.isCancelled, model.session?.token == token { model.errorMessage = error.localizedDescription } }
    }
    private func remove(_ entry: JSONValue) async {
        await model.perform {
            guard let client = model.client else { return }
            _ = try await model.request("wellness/entries/delete", body: .object(["record_id": entry["record_id"], "version": .number((entry["version"].numberValue ?? 0) + 1), "batch_id": .string(UUID().uuidString), "kind": entry["entry"]["kind"]]))
        }
        deleting = nil
        await reload()
    }
}

struct LogEditor: View {
    var model: AppModel
    let existing: JSONValue?
    let initialKind: String
    let initialValues: [String: String]
    let initialDate: Date
    @Environment(\.dismiss) private var dismiss
    @State private var kind = "journal"
    @State private var values: [String: String] = [:]
    @State private var date = Date()
    @State private var recordID = UUID().uuidString
    @State private var failure: String?
    @State private var saving = false
    var body: some View {
        Form {
            Picker("Log type", selection: $kind) { ForEach(EntryDraft.kinds, id: \.self) { Text($0.replacingOccurrences(of: "_", with: " ").capitalized).tag($0) } }.disabled(existing != nil)
            DatePicker(kind == "training" ? "Session started at" : "Observed at", selection: $date)
            ForEach(EntryDraft.fields(kind), id: \.key) { field in
                let binding = Binding(get: { values[field.key] ?? field.choices.first ?? "" }, set: { values[field.key] = $0 })
                if field.format == "bool" {
                    Toggle(field.label, isOn: Binding(get: { binding.wrappedValue == "true" }, set: { binding.wrappedValue = $0 ? "true" : "false" }))
                } else if field.format == "nutrients" {
                    Text(field.key == "nutrients_per_100g" ? "Amounts per 100 grams" : field.key == "daily_targets" ? "Your daily targets" : field.key == "totals" ? "Full-day equivalent totals" : "Additional nutrients for this portion").font(.headline)
                    NumberMapEditor(value: binding, definitions: kind == "diet_quality" ? NumberMapEditor.dietFields : NumberMapEditor.nutrients.filter { field.key != "micronutrients" || !["energy_kcal", "protein_g", "carbohydrate_g", "fat_g", "fiber_g", "water_ml"].contains($0.0) })
                } else if field.format == "ingredients" {
                    RecipeIngredientsEditor(model: model, value: binding)
                } else if !field.choices.isEmpty {
                    Picker(field.label, selection: binding) { ForEach(field.choices, id: \.self) { Text($0.capitalized).tag($0) } }
                } else if field.format.contains("datetime") {
                    DatePicker(field.label, selection: Binding(get: { ISO8601DateFormatter().date(from: binding.wrappedValue) ?? Date() }, set: { binding.wrappedValue = ISO8601DateFormatter().string(from: $0) }))
                    Button(binding.wrappedValue.isEmpty ? "Record this time" : "Clear recorded time") {
                        binding.wrappedValue = binding.wrappedValue.isEmpty ? ISO8601DateFormatter().string(from: Date()) : ""
                    }
                    Text(binding.wrappedValue.isEmpty ? "Not recorded" : binding.wrappedValue).font(.caption)
                } else if field.format.contains("number") {
                    TextField(field.label, text: binding).keyboardType(.decimalPad)
                } else {
                    TextField(field.label, text: binding, axis: .vertical).lineLimit(1...6)
                }
            }
            if kind == "training" { Text("Record effort for the whole session on the CR10 scale, ideally about 30 minutes afterward. External weight excludes body mass.").font(.caption) }
            if let failure { Text(failure).foregroundStyle(.red) }
            Button("Save log") { Task { await save() } }.disabled(saving)
        }.disabled(saving).navigationTitle(existing == nil ? "New observation" : "Edit observation")
            .toolbar { Button("Cancel") { dismiss() }.disabled(saving) }
            .onAppear {
                kind = initialKind; values = initialValues; date = initialDate
                if let existing {
                    kind = existing["entry"]["kind"].stringValue
                    values = EntryDraft.decode(existing["entry"])
                    date = Date(timeIntervalSince1970: existing["at"].numberValue ?? 0)
                    recordID = existing["record_id"].stringValue
                }
            }
    }
    private func save() async {
        guard let client = model.client, let token = model.session?.token else { return }
        saving = true; failure = nil
        defer { saving = false }
        do {
            let entry = try EntryDraft.encode(kind: kind, values: values)
            _ = try await model.request("wellness/entries/save", body: .object(["record_id": .string(recordID), "version": .number((existing?["version"].numberValue ?? 0) + 1), "batch_id": .string(UUID().uuidString), "at": .number(floor(date.timeIntervalSince1970)), "timezone": .string(TimeZone.current.identifier), "entry": entry]))
            guard model.session?.token == token else { return }
            dismiss()
        } catch { if model.session?.token == token { failure = error.localizedDescription } }
    }
}
