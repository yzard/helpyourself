import SwiftUI

struct ObservationEditor: View {
    let observation: JSONValue
    let metrics: [JSONValue]
    let pageCount: Int
    let save: (JSONValue) async throws -> Void
    @State private var payload: JSONValue
    @State private var status: String
    @State private var saving = false
    @State private var error: String?
    @Environment(\.dismiss) private var dismiss
    init(observation: JSONValue, metrics: [JSONValue], pageCount: Int, save: @escaping (JSONValue) async throws -> Void) {
        self.observation = observation; self.metrics = metrics; self.pageCount = pageCount; self.save = save
        _payload = State(initialValue: observation == .null ? .object(["source": .object(["page": .number(1), "quote": .string("")])]) : observation["payload"])
        _status = State(initialValue: observation["status"].stringValue.isEmpty ? "pending" : observation["status"].stringValue)
    }
    var body: some View {
        NavigationStack {
            Form {
                Section("As printed on the report") {
                    field("Name", "raw_name"); field("Result, including < or >", "raw_result"); field("Unit", "raw_unit")
                    field("Reference range", "reference_range"); field("Report flag", "report_flag")
                    field("Collection date: YYYY-MM-DD or ISO timestamp", "sampled_at")
                }
                Section("Comparison") {
                    Picker("Metric", selection: text("metric_id")) {
                        Text("Unmapped — retain original").tag("")
                        ForEach(metrics, id: \.identifier) { Text($0["name"].stringValue).tag($0["metric_id"].stringValue) }
                    }
                    Text("Check the printed test, value and unit before mapping. The server handles supported unit conversions; unknown units and bounded or text results remain in the archive without an exact numeric trend.").font(.caption)
                }
                Section("Source") {
                    Stepper("Page \(Int(payload["source"]["page"].numberValue ?? 1))", value: Binding(get: { Int(payload["source"]["page"].numberValue ?? 1) }, set: { payload["source"]["page"] = .number(Double($0)) }), in: 1...max(1, pageCount))
                    TextField("Exact source text", text: Binding(get: { payload["source"]["quote"].stringValue }, set: { payload["source"]["quote"] = .string($0) }), axis: .vertical)
                    field("Your notes", "notes")
                }
                Section("Review decision") {
                    Picker("Status", selection: $status) { Text("Pending").tag("pending"); Text("Confirmed").tag("confirmed"); Text("Rejected").tag("rejected") }
                    Text("Confirm after checking the original report. Each save retains the previous revision.").font(.caption)
                }
                if let error { Section { Text(error).foregroundStyle(.red) } }
            }.navigationTitle(observation == .null ? "Add result" : "Review result")
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() }.disabled(saving) }
                    ToolbarItem(placement: .confirmationAction) { Button("Save") { Task {
                        saving = true; defer { saving = false }
                        do {
                            try await save(.object(["observation_id": observation["observation_id"], "expected_revision": observation["revision"], "status": .string(status), "payload": payload]))
                            dismiss()
                        } catch { self.error = error.localizedDescription }
                    } }.disabled(saving || payload["raw_name"].stringValue.isEmpty || payload["raw_result"].stringValue.isEmpty) }
                }.interactiveDismissDisabled(saving)
        }
    }
    private func text(_ key: String) -> Binding<String> { Binding(get: { payload[key].stringValue }, set: { payload[key] = ["raw_name", "raw_result"].contains(key) ? .string($0) : .optional($0) }) }
    private func field(_ label: String, _ key: String) -> some View { TextField(label, text: text(key), axis: .vertical).autocorrectionDisabled() }
}

struct ContextEditor: View {
    let save: (JSONValue) async throws -> Void
    @State private var context: JSONValue
    @State private var fasting: String
    @State private var error: String?
    @State private var saving = false
    @Environment(\.dismiss) private var dismiss
    init(context: JSONValue, save: @escaping (JSONValue) async throws -> Void) {
        self.save = save; _context = State(initialValue: context)
        _fasting = State(initialValue: context["fasting"] == .null ? "unknown" : (context["fasting"].boolValue ? "yes" : "no"))
    }
    var body: some View {
        NavigationStack { Form {
            Picker("Fasting", selection: $fasting) { Text("Unknown").tag("unknown"); Text("Yes").tag("yes"); Text("No").tag("no") }
            field("Recent exercise", "recent_exercise"); field("Illness", "illness"); field("Medications", "medications"); field("Notes", "notes")
            if let error { Text(error).foregroundStyle(.red) }
        }.navigationTitle("Collection context").toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() }.disabled(saving) }
            ToolbarItem(placement: .confirmationAction) { Button("Save") { Task {
                saving = true; defer { saving = false }
                context["fasting"] = fasting == "unknown" ? .null : .bool(fasting == "yes")
                do { try await save(context); dismiss() } catch { self.error = error.localizedDescription }
            } }.disabled(saving) }
        }.interactiveDismissDisabled(saving) }
    }
    private func field(_ label: String, _ key: String) -> some View { TextField(label, text: Binding(get: { context[key].stringValue }, set: { context[key] = .optional($0) }), axis: .vertical) }
}
