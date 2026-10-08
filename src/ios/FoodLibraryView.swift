import SwiftUI

struct FoodLibraryView: View {
    var model: AppModel
    @State private var kind = "food"
    @State private var query = ""
    @State private var entries: [JSONValue] = []
    @State private var editor: Selection?
    @State private var failure: String?
    private struct Selection: Identifiable { let id = UUID(); let existing: JSONValue?; let kind: String }
    var body: some View {
        List {
            Section {
                Picker("Library", selection: $kind) { Text("Foods").tag("food"); Text("Recipes").tag("recipe"); Text("Goals").tag("nutrition_goals") }
                NavigationLink("Look up a barcode", destination: FoodLookupView(model: model))
                Button("Add entry") { editor = Selection(existing: nil, kind: kind) }
                Text("Your local food database retains label sources. Review portions before saving. Missing nutrients remain unknown.").font(.caption)
                Text("Nutrient names include energy_kcal, protein_g, carbohydrate_g, fat_g, fiber_g, water_ml, sodium_mg, calcium_mg, iron_mg and vitamin_d_ug.").font(.caption)
                if let failure { Text(failure).foregroundStyle(.secondary) }
            }
            ForEach(Array(entries.filter { query.isEmpty || $0["entry"]["content"].selfDescription.localizedCaseInsensitiveContains(query) }.enumerated()), id: \.offset) { _, item in
                Section {
                    Text(item["entry"]["content"]["name"].stringValue.isEmpty ? "Daily targets" : item["entry"]["content"]["name"].stringValue).font(.headline)
                    Text(item["entry"]["content"]["source"].stringValue).font(.caption)
                    Button("Edit") { editor = Selection(existing: item, kind: kind) }
                    if kind != "nutrition_goals" { NavigationLink("Measure a portion") { FoodPortionView(model: model, item: item) } }
                    Button("Delete", role: .destructive) { Task { await remove(item) } }
                }
            }
        }.navigationTitle("Food library").searchable(text: $query, prompt: "Name, brand or barcode")
            .task(id: kind) { await load() }.refreshable { await load() }
            .sheet(item: $editor, onDismiss: { Task { await load() } }) { selection in
                NavigationStack { LogEditor(model: model, existing: selection.existing, initialKind: selection.kind, initialValues: selection.kind == "recipe" ? ["ingredients": "[]"] : [:], initialDate: Date()) }
            }
    }
    private func load() async {
        guard let client = model.client, let token = model.session?.token else { return }
        do {
            let response = try await model.request("wellness/library", body: .object(["kind": .string(kind)]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            entries = response["entries"].arrayValue; failure = nil
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

private struct FoodPortionView: View {
    var model: AppModel
    let item: JSONValue
    @State private var amount = ""
    @State private var meal = "snack"
    @State private var result: JSONValue = .null
    @State private var failure: String?
    @State private var loading = false
    private var unit: String { item["entry"]["kind"] == .string("food") ? "g" : "servings" }
    var body: some View {
        Form {
            Text(item["entry"]["content"]["name"].stringValue)
            TextField("Amount in " + unit, text: $amount).keyboardType(.decimalPad)
            Picker("Meal", selection: $meal) { ForEach(["breakfast", "lunch", "dinner", "snack", "drink"], id: \.self) { Text($0.capitalized).tag($0) } }
            Button("Calculate portion") { Task { await calculate() } }.disabled(loading)
            if let failure { Text(failure).foregroundStyle(.secondary) }
            if result != .null {
                ForEach(result["nutrients"].objectValue.keys.sorted(), id: \.self) { key in LabeledContent(key, value: result["nutrients"][key].numberValue?.formatted() ?? "Unknown") }
                Text("Incomplete nutrients: " + result["incomplete_nutrients"].arrayValue.map(\.stringValue).joined(separator: ", ")).font(.caption)
                NavigationLink("Review and save food log") { LogEditor(model: model, existing: nil, initialKind: "nutrition", initialValues: draft(), initialDate: Date()) }
            }
        }.navigationTitle("Food portion").onChange(of: amount) { _, _ in result = .null }.onChange(of: meal) { _, _ in result = .null }
    }
    private func draft() -> [String: String] {
        var values = ["food": result["food"].stringValue, "meal": result["meal"].stringValue]
        let macros = ["energy_kcal", "protein_g", "carbohydrate_g", "fat_g", "fiber_g", "water_ml"]
        for key in macros { values[key] = result["nutrients"][key].numberValue.map { String($0) } ?? "" }
        values["micronutrients"] = result["nutrients"].objectValue.keys.sorted().filter { !macros.contains($0) && result["nutrients"][$0].numberValue != nil }.map { "\($0) = \(result["nutrients"][$0].numberValue ?? 0)" }.joined(separator: "\n")
        let origin = JSONValue.object(["basis": result["basis"], "source_snapshot": result["source_snapshot"]])
        if let data = try? JSONEncoder().encode(origin) { values["_origin"] = String(data: data, encoding: .utf8) }
        return values
    }
    private func calculate() async {
        guard let client = model.client, let token = model.session?.token, let number = Double(amount), number.isFinite, number > 0 else { failure = "Enter a positive portion."; return }
        loading = true; defer { loading = false }
        do {
            let response = try await model.request("wellness/food/portion", body: .object(["basis": .object(["record_id": item["record_id"], "version": item["version"], "amount": .number(number), "unit": .string(unit)]), "meal": .string(meal)]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            result = response; failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
}

struct RecipeIngredientsEditor: View {
    var model: AppModel
    @Binding var value: String
    @State private var foods: [JSONValue] = []
    @State private var selected = ""
    @State private var grams = ""
    @State private var failure: String?
    private var ingredients: [JSONValue] { guard let data = value.data(using: .utf8) else { return [] }; return (try? JSONDecoder().decode([JSONValue].self, from: data)) ?? [] }
    var body: some View {
        Section("Ingredient snapshots") {
            ForEach(Array(ingredients.enumerated()), id: \.offset) { index, item in
                HStack { Text(item["food_name"].stringValue + " · " + (item["grams"].numberValue?.formatted() ?? "") + " g"); Spacer(); Button("Remove") { var items = ingredients; items.remove(at: index); save(items) } }
            }
            Picker("Food", selection: $selected) {
                Text("Select a saved food").tag("")
                ForEach(Array(foods.enumerated()), id: \.offset) { _, food in Text(food["entry"]["content"]["name"].stringValue).tag(food["record_id"].stringValue) }
            }
            TextField("Ingredient grams", text: $grams).keyboardType(.decimalPad)
            Button("Add ingredient snapshot") {
                guard let food = foods.first(where: { $0["record_id"].stringValue == selected }), let number = Double(grams), number.isFinite, number > 0 else { failure = "Select a food and enter positive grams."; return }
                let content = food["entry"]["content"]
                var items = ingredients
                items.append(.object(["food_name": content["name"], "grams": .number(number), "nutrients_per_100g": content["nutrients_per_100g"], "source": .string(content["source"].stringValue + " · " + food["record_id"].stringValue + " revision " + String(Int(food["version"].numberValue ?? 0)))]))
                save(items); grams = ""; failure = nil
            }
            if let failure { Text(failure).font(.caption) }
        }.task {
            guard let client = model.client, let token = model.session?.token else { return }
            do { let response = try await model.request("wellness/library", body: .object(["kind": .string("food")]))
                guard !Task.isCancelled, model.session?.token == token else { return }; foods = response["entries"].arrayValue
            } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
        }
    }
    private func save(_ items: [JSONValue]) { if let data = try? JSONEncoder().encode(items), let text = String(data: data, encoding: .utf8) { value = text } }
}
