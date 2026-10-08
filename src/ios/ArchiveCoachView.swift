import SwiftUI

struct ArchiveCoachView: View {
    var model: AppModel
    @State private var question = ""
    @State private var style = "concise"
    @State private var allowDrafts = false
    @State private var date = Date()
    @State private var runID: String?
    @State private var failure: String?
    @State private var submitting = false
    var body: some View {
        List {
            Section("Ask about your archive") {
                NavigationLink("Editable memory", destination: CoachMemoryView(model: model))
                DatePicker("Context date", selection: $date, displayedComponents: .date)
                Picker("Communication style", selection: $style) { ForEach(["concise", "detailed", "direct", "gentle"], id: \.self) { Text($0.capitalized).tag($0) } }
                Toggle("Include unsubmitted plan drafts", isOn: $allowDrafts)
                TextField("Question", text: $question, axis: .vertical).lineLimit(3...10)
                VoiceQuestionButton(question: $question)
                Text("The answer uses source-separated daily data, recent logs, confirmed reports and enabled memory. Claims remain unverified. Drafts do not change your records.").font(.caption)
                Button("Submit question") { Task { await submit() } }.disabled(submitting || question.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !model.capabilities["analysis"].boolValue)
                if let failure { Text(failure).foregroundStyle(.secondary) }
            }
            if let runID { NavigationLink("Open answer and follow up") { CoachAnswerView(model: model, runID: runID) } }
        }.navigationTitle("Archive questions")
    }
    private func submit() async {
        guard let client = model.client, let token = model.session?.token else { return }
        submitting = true; defer { submitting = false }
        let formatter = DateFormatter(); formatter.locale = Locale(identifier: "en_US_POSIX"); formatter.dateFormat = "yyyy-MM-dd"
        do {
            let response = try await model.request("wellness/coach", body: .object(["question": .string(question), "date": .string(formatter.string(from: date)), "timezone": .string(TimeZone.current.identifier), "style": .string(style), "prior_run_id": .null, "allow_drafts": .bool(allowDrafts)]))
            guard model.session?.token == token else { return }; runID = response["run_id"].stringValue; failure = nil
        } catch { if model.session?.token == token { failure = error.localizedDescription } }
    }
}

struct CoachAnswerView: View {
    var model: AppModel
    let runID: String
    @State private var run: JSONValue = .null
    @State private var followUp = ""
    @State private var nextID: String?
    @State private var draft: DraftSelection?
    @State private var failure: String?
    private struct DraftSelection: Identifiable { let id = UUID(); let entry: JSONValue }
    var body: some View {
        List {
            Section {
                Text("Unverified · " + run["status"].stringValue)
                Text(run["input"]["question"].stringValue)
                if run["status"] == .string("stale") { Text("Source data changed. Start a new question with the current archive.") }
                if let failure { Text(failure).foregroundStyle(.secondary) }
                Button("Refresh") { Task { await load() } }
            }
            ForEach(Array(run["output"]["coach"]["claims"].arrayValue.enumerated()), id: \.offset) { _, claim in
                Section {
                    Text(claim["text"].stringValue)
                    ForEach(claim["evidence_ids"].arrayValue.map(\.stringValue), id: \.self) { id in
                        if let evidence = run["input"]["evidence"].arrayValue.first(where: { $0["id"].stringValue == id }) {
                            DisclosureGroup("Source: " + id) { Text(evidence["value"].selfDescription).font(.caption).textSelection(.enabled) }
                        }
                    }
                }
            }
            Section("Missing information") { ForEach(run["output"]["coach"]["missing_information"].arrayValue.map(\.stringValue), id: \.self) { Text($0) } }
            Section("Questions to consider") { ForEach(run["output"]["coach"]["questions"].arrayValue.map(\.stringValue), id: \.self) { Text($0) } }
            Section("Unsubmitted drafts") {
                ForEach(Array(run["output"]["coach"]["drafts"].arrayValue.enumerated()), id: \.offset) { _, item in
                    Button("Review: " + item["title"].stringValue) { draft = DraftSelection(entry: item["entry"]) }.disabled(run["status"] != .string("ready"))
                }
            }
            if run["status"] == .string("ready") {
                Section("Follow up") {
                    TextField("Follow-up question", text: $followUp, axis: .vertical)
                    Button("Submit follow-up") { Task { await ask() } }.disabled(followUp.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    if let nextID { NavigationLink("Open follow-up answer") { CoachAnswerView(model: model, runID: nextID) } }
                }
            }
        }.navigationTitle("Archive answer")
            .task(id: runID) {
                repeat {
                    await load()
                    guard !Task.isCancelled, ["queued", "running"].contains(run["status"].stringValue) else { return }
                    do { try await Task.sleep(for: .seconds(2)) } catch { return }
                } while !Task.isCancelled
            }
            .sheet(item: $draft) { item in
                NavigationStack { LogEditor(model: model, existing: nil, initialKind: item.entry["kind"].stringValue, initialValues: EntryDraft.decode(item.entry), initialDate: Date()) }
            }
    }
    private func load() async {
        guard let client = model.client, let token = model.session?.token else { return }
        do { let response = try await model.request("analysis/get", body: .object(["run_id": .string(runID)]))
            guard !Task.isCancelled, model.session?.token == token else { return }; run = response; failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
    private func ask() async {
        guard let client = model.client, let token = model.session?.token else { return }
        do {
            let response = try await model.request("wellness/coach", body: .object(["question": .string(followUp), "date": run["input"]["scope"]["date"], "timezone": run["input"]["scope"]["timezone"], "style": run["input"]["style"], "prior_run_id": .string(runID), "allow_drafts": run["input"]["allow_drafts"]]))
            guard model.session?.token == token else { return }; nextID = response["run_id"].stringValue; failure = nil
        } catch { if model.session?.token == token { failure = error.localizedDescription } }
    }
}

private struct CoachMemoryView: View {
    var model: AppModel
    @State private var items: [JSONValue] = []
    @State private var selected: Selection?
    private struct Selection: Identifiable { let id = UUID(); let item: JSONValue? }
    var body: some View {
        List {
            Button("Add memory") { selected = Selection(item: nil) }
            Text("Enabled memories provide preferences and context. They do not become confirmed clinical facts. Deletion invalidates saved analyses that used the archive.").font(.caption)
            ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                Section(item["entry"]["content"]["name"].stringValue) {
                    Text(item["entry"]["content"]["content"].stringValue)
                    Text(item["entry"]["content"]["enabled"].boolValue ? "Enabled" : "Disabled").font(.caption)
                    Button("Edit") { selected = Selection(item: item) }
                    Button("Delete", role: .destructive) { Task { await remove(item) } }
                }
            }
        }.navigationTitle("Editable memory").task { await load() }
            .sheet(item: $selected, onDismiss: { Task { await load() } }) { selection in
                NavigationStack { LogEditor(model: model, existing: selection.item, initialKind: "memory", initialValues: [:], initialDate: Date()) }
            }
    }
    private func load() async {
        guard let client = model.client, let token = model.session?.token else { return }
        do { let response = try await model.request("wellness/library", body: .object(["kind": .string("memory")]))
            guard !Task.isCancelled, model.session?.token == token else { return }; items = response["entries"].arrayValue
        } catch { if model.session?.token == token { model.errorMessage = error.localizedDescription } }
    }
    private func remove(_ item: JSONValue) async {
        await model.perform {
            guard let client = model.client else { return }
            _ = try await model.request("wellness/entries/delete", body: .object(["record_id": item["record_id"], "version": .number((item["version"].numberValue ?? 0) + 1), "batch_id": .string(UUID().uuidString), "kind": .string("memory")]))
        }
        await load()
    }
}
