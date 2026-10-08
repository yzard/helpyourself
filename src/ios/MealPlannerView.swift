import SwiftUI

struct MealPlannerView: View {
    var model: AppModel
    @State private var date = Date()
    @State private var result: JSONValue = .null
    @State private var recipes: [JSONValue] = []
    @State private var failure: String?
    @State private var editing: Selection?
    @State private var deleting: JSONValue?
    private struct Selection: Identifiable { let id = UUID(); let existing: JSONValue?; let values: [String: String] }
    var body: some View {
        List {
            Section("Seven-day plan") {
                DatePicker("Starting date", selection: $date, displayedComponents: .date)
                Text("Planned meals do not count as actual intake. Choose a date when saving each plan.").font(.caption)
                if let failure { Text(failure).foregroundStyle(.secondary) }
            }
            Section("Plan from a saved recipe") {
                ForEach(Array(recipes.enumerated()), id: \.offset) { _, recipe in
                    Button(recipe["entry"]["content"]["name"].stringValue) {
                        var values = EntryDraft.decode(recipe["entry"])
                        values["planned_servings"] = "1"; values["status"] = "planned"
                        editing = Selection(existing: nil, values: values)
                    }
                }
                NavigationLink("Manage recipes", destination: FoodLibraryView(model: model))
            }
            Section("Planned meals") {
                ForEach(Array(result["plans"].arrayValue.enumerated()), id: \.offset) { _, plan in
                    VStack(alignment: .leading) {
                        Text(plan["entry"]["content"]["recipe"]["name"].stringValue)
                        Text(Date(timeIntervalSince1970: plan["at"].numberValue ?? 0).formatted())
                        Text("\(plan["entry"]["content"]["planned_servings"].numberValue?.formatted() ?? "Unknown") servings · \(plan["entry"]["content"]["status"].stringValue)").font(.caption)
                        Button("Edit or skip") { editing = Selection(existing: plan, values: [:]) }
                        Button("Delete plan", role: .destructive) { deleting = plan }
                    }
                }
            }
            Section("Shopping quantities") {
                ForEach(Array(result["shopping"].arrayValue.enumerated()), id: \.offset) { _, item in
                    VStack(alignment: .leading) {
                        Text("\(item["food"].stringValue): \(item["grams"].numberValue?.formatted() ?? "Unknown") g")
                        Text(item["source"].stringValue).font(.caption)
                    }
                }
                if !result["shopping"].arrayValue.isEmpty {
                    ShareLink("Share shopping list", item: result["shopping"].arrayValue.map { "\($0["food"].stringValue): \($0["grams"].numberValue?.formatted() ?? "Unknown") g (\($0["source"].stringValue))" }.joined(separator: "\n"))
                }
            }
        }.confirmationDialog("Delete this meal plan?", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })) {
            Button("Delete", role: .destructive) { if let plan = deleting { Task { await remove(plan) } } }
        }.navigationTitle("Meal planner").task(id: date) { await load() }.refreshable { await load() }
            .sheet(item: $editing, onDismiss: { Task { await load() } }) { selection in NavigationStack { LogEditor(model: model, existing: selection.existing, initialKind: "meal_plan", initialValues: selection.values, initialDate: date) } }
    }
    private func remove(_ plan: JSONValue) async {
        guard let client = model.client, let token = model.session?.token else { return }
        do {
            _ = try await model.request("wellness/entries/delete", body: .object(["record_id": plan["record_id"], "version": .number((plan["version"].numberValue ?? 0) + 1), "batch_id": .string(UUID().uuidString), "kind": .string("meal_plan")]))
            guard model.session?.token == token else { return }
            deleting = nil; await load()
        } catch { if model.session?.token == token { failure = error.localizedDescription } }
    }
    private func load() async {
        guard let client = model.client, let token = model.session?.token else { return }
        let start = Calendar.current.startOfDay(for: date)
        guard let end = Calendar.current.date(byAdding: .day, value: 7, to: start) else { return }
        do {
            let response = try await model.request("wellness/meals", body: .object(["start_at": .number(start.timeIntervalSince1970), "end_at": .number(end.timeIntervalSince1970)]))
            let library = try await model.request("wellness/library", body: .object(["kind": .string("recipe")]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            result = response; recipes = library["entries"].arrayValue; failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
}
