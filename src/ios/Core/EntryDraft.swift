import Foundation

public nonisolated struct EntryField: Sendable {
    public let key: String
    public let label: String
    public let format: String
    public let choices: [String]
    public init(_ key: String, _ label: String, _ format: String, _ choices: [String]) {
        self.key = key; self.label = label; self.format = format; self.choices = choices
    }
}

public nonisolated enum EntryDraft {
    public static let kinds = ["meal_plan", "diet_quality", "memory", "reminder", "food", "recipe", "nutrition_goals", "exercise", "workout_template", "planned_workout", "journal", "training_day", "training", "nutrition", "body", "blood_pressure", "cycle", "cycle_prediction", "breathing"]
    public static func fields(_ kind: String) -> [EntryField] {
        let fields: [EntryField]
        switch kind {
        case "journal": fields = [EntryField("mood", "Mood (0–10)", "number", []), EntryField("perceived_stress", "Perceived stress (0–10)", "number", []), EntryField("behaviors", "Behaviors: name = yes or no, one per line", "behaviors", []), EntryField("measurements", "Measurements: name = number unit, one per line", "measurements", []), EntryField("times", "Times: name = HH:MM, one per line", "times", [])]
        case "memory": fields = [EntryField("name", "Memory name", "required", []), EntryField("content", "Preference or context", "required", []), EntryField("enabled", "Enabled", "bool", ["true", "false"])]
        case "reminder": fields = [EntryField("title", "Reminder title", "required", []), EntryField("body", "Reminder message", "text", []), EntryField("enabled", "Enabled", "bool", ["true", "false"]), EntryField("recurrence", "Recurrence", "choice", ["once", "daily", "weekly"]), EntryField("start_date", "Start date YYYY-MM-DD", "required", []), EntryField("end_date", "End date YYYY-MM-DD (optional)", "optional-text", []), EntryField("local_time", "Local time HH:MM", "required", []), EntryField("weekdays", "Weekly days, one per line: Monday=1, Sunday=7", "integers", []), EntryField("quiet_start_minute", "Quiet start, minutes after midnight (optional)", "number", []), EntryField("quiet_end_minute", "Quiet end, minutes after midnight (optional)", "number", [])]
        case "food": fields = [EntryField("name", "Food name", "required", []), EntryField("brand", "Brand", "text", []), EntryField("barcode", "Barcode (optional)", "optional-text", []), EntryField("source", "Source or label", "required", []), EntryField("nutrients_per_100g", "Nutrients per 100 g: name = value, one per line", "nutrients", [])]
        case "recipe": fields = [EntryField("name", "Recipe name", "required", []), EntryField("servings", "Servings", "required-number", []), EntryField("ingredients", "Ingredients", "ingredients", []), EntryField("instructions", "Instructions", "text", [])]
        case "meal_plan": return Self.fields("recipe") + [EntryField("planned_servings", "Planned servings", "required-number", []), EntryField("status", "Plan status", "choice", ["planned", "skipped"])]
        case "diet_quality": fields = [EntryField("source", "Dietary equivalent source and version", "required", []), EntryField("complete_day", "Full day confirmed", "bool", []), EntryField("age_two_or_older", "For a person aged two or older", "bool", []), EntryField("totals", "Confirmed equivalents: name = value, one per line", "nutrients", [])]
        case "nutrition_goals": fields = [EntryField("daily_targets", "Daily targets: nutrient name = value", "nutrients", [])]
        case "exercise": fields = [EntryField("name", "Exercise name", "required", []), EntryField("equipment", "Equipment", "required", []), EntryField("muscle_groups", "Muscle groups, one per line", "lines", []), EntryField("instructions", "Instructions", "text", [])]
        case "workout_template": fields = [EntryField("name", "Template name", "required", []), EntryField("blocks", "Exercise, equipment, sets, repetitions, seconds, kg, rest seconds. Leave repetitions or seconds blank if unused.", "prescriptions", [])]
        case "planned_workout": fields = [EntryField("title", "Workout title", "required", []), EntryField("duration_minutes", "Planned duration (minutes)", "required-number", []), EntryField("status", "Plan status", "choice", ["planned", "skipped"]), EntryField("blocks", "Exercise, equipment, sets, repetitions, seconds, kg, rest seconds. Leave repetitions or seconds blank if unused.", "prescriptions", [])]
        case "training_day": fields = [EntryField("status", "Training day confirmation (selected local date)", "choice", ["rest", "all_sessions_logged"])]
        case "training": fields = [EntryField("activity", "Activity", "required", []), EntryField("duration_minutes", "Duration (minutes)", "required-number", []), EntryField("ended_at", "Session ended at", "required-datetime", []), EntryField("paused_minutes", "Paused minutes (0 if none)", "required-number", []), EntryField("duration_basis", "Duration basis", "choice", ["elapsed_including_pauses", "active_excluding_pauses"]), EntryField("rpe_answered_at", "Effort answered at (required with CR10)", "datetime", []), EntryField("rpe_cr10", "Session effort (CR10, 0–10)", "number", []), EntryField("sets", "Sets: exercise, repetitions, kg, one per line", "sets", [])]
        case "nutrition": fields = [EntryField("food", "Food or drink", "required", []), EntryField("meal", "Meal", "required", []), EntryField("energy_kcal", "Energy (kcal)", "number", []), EntryField("protein_g", "Protein (g)", "number", []), EntryField("carbohydrate_g", "Carbohydrate (g)", "number", []), EntryField("fat_g", "Fat (g)", "number", []), EntryField("fiber_g", "Fiber (g)", "number", []), EntryField("water_ml", "Water (ml)", "number", []), EntryField("micronutrients", "Other nutrients: name = value", "nutrients", [])]
        case "body": fields = [EntryField("weight_kg", "Weight (kg)", "required-number", []), EntryField("body_fat_percent", "Body fat (%)", "number", []), EntryField("waist_cm", "Waist (cm)", "number", [])]
        case "blood_pressure": fields = [EntryField("systolic_mmhg", "Systolic (mmHg)", "required-number", []), EntryField("diastolic_mmhg", "Diastolic (mmHg)", "required-number", []), EntryField("pulse_bpm", "Pulse (bpm)", "number", []), EntryField("arm", "Arm", "choice", ["unknown", "left", "right"]), EntryField("posture", "Posture", "choice", ["unknown", "seated", "standing", "supine"])]
        case "cycle_prediction": fields = [EntryField("start_date", "Source predicted start (YYYY-MM-DD)", "required", []), EntryField("end_date", "Source predicted end (YYYY-MM-DD)", "required", []), EntryField("generated_at", "Source prediction generated at", "required-datetime", []), EntryField("source", "Prediction source and version", "required", []), EntryField("uncertainty", "Source uncertainty or accuracy statement", "required", [])]
        case "cycle": fields = [EntryField("flow", "Flow", "choice", ["none", "spotting", "light", "medium", "heavy"]), EntryField("context", "Context", "choice", ["cycle", "pregnancy", "postpartum", "perimenopause", "unknown"]), EntryField("symptoms", "Symptoms, one per line", "lines", [])]
        case "breathing": fields = [EntryField("duration_minutes", "Duration (minutes)", "required-number", []), EntryField("breaths_per_minute", "Breaths per minute (optional)", "number", [])]
        default: return []
        }
        return fields + [EntryField("note", "Notes", "text", [])]
    }
    public static func encode(kind: String, values: [String: String]) throws -> JSONValue {
        guard kinds.contains(kind) else { throw APIError.invalidResponse }
        var content: [String: JSONValue] = [:]
        for field in fields(kind) {
            let value = (values[field.key] ?? field.choices.first ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
            if field.format.hasPrefix("required") && value.isEmpty { throw DraftError.message("Enter \(field.label).") }
            if field.format.contains("datetime") {
                if value.isEmpty {
                    guard !field.format.hasPrefix("required") else { throw DraftError.message("Enter " + field.label + ".") }
                    content[field.key] = .null
                } else {
                    guard let date = ISO8601DateFormatter().date(from: value) else { throw DraftError.message("Enter an ISO 8601 date with a time zone.") }
                    content[field.key] = .number(floor(date.timeIntervalSince1970))
                }
            } else if field.format.contains("number") {
                if value.isEmpty { content[field.key] = .null }
                else {
                    guard let number = Double(value), number.isFinite else { throw DraftError.message("Enter a number for \(field.label).") }
                    content[field.key] = .number(number)
                }
            } else if field.format == "bool" { content[field.key] = .bool(value == "true")
            } else if field.format == "integers" { content[field.key] = .array(try lines(value).map { text in guard let number = Int(text) else { throw DraftError.message("Enter whole day numbers.") }; return .number(Double(number)) })
            } else if field.format == "optional-text" { content[field.key] = value.isEmpty ? .null : .string(value)
            } else if field.format == "ingredients" {
                guard let data = value.data(using: .utf8), let array = try? JSONDecoder().decode([JSONValue].self, from: data) else { throw DraftError.message("Use the recipe ingredient editor.") }
                content[field.key] = .array(array)
            } else if field.format == "nutrients" {
                var nutrients: [String: JSONValue] = [:]
                for line in lines(value) {
                    let parts = line.components(separatedBy: "=").map { $0.trimmingCharacters(in: .whitespaces) }
                    guard parts.count == 2, !parts[0].isEmpty, let number = Double(parts[1]), number.isFinite, number >= 0, nutrients[parts[0]] == nil else { throw DraftError.message("Use a unique nutrient name = nonnegative value on each line.") }
                    nutrients[parts[0]] = .number(number)
                }
                content[field.key] = .object(nutrients)
            } else if field.format == "prescriptions" {
                content[field.key] = .array(try lines(value).map { line in
                    let parts = line.components(separatedBy: ",").map { $0.trimmingCharacters(in: .whitespaces) }
                    guard parts.count == 7, !parts[0].isEmpty, !parts[1].isEmpty else { throw DraftError.message("Each block needs seven comma-separated fields.") }
                    var block: [String: JSONValue] = ["exercise": .string(parts[0]), "equipment": .string(parts[1])]
                    for (index, key) in ["sets", "repetitions", "duration_seconds", "external_weight_kg", "rest_seconds"].enumerated() {
                        let text = parts[index + 2]
                        if text.isEmpty { block[key] = .null } else {
                            guard let number = Double(text), number.isFinite, number >= 0 else { throw DraftError.message("Use nonnegative numeric prescription fields.") }
                            block[key] = .number(number)
                        }
                    }
                    return .object(block)
                })
            } else if field.format == "sets" {
                content[field.key] = .array(try lines(value).map { line in
                    let parts = line.components(separatedBy: ",").map { $0.trimmingCharacters(in: .whitespaces) }
                    guard parts.count == 3, !parts[0].isEmpty, let repetitions = Int(parts[1]), repetitions > 0,
                          let weight = Double(parts[2]), weight.isFinite, weight >= 0 else { throw DraftError.message("Each set needs an exercise, repetitions and kg.") }
                    return .object(["exercise": .string(parts[0]), "repetitions": .number(Double(repetitions)), "external_weight_kg": .number(weight)])
                })
            } else if field.format == "behaviors" {
                var behaviors: [String: JSONValue] = [:]
                for line in lines(value) {
                    let parts = line.components(separatedBy: "=").map { $0.trimmingCharacters(in: .whitespaces) }
                    guard parts.count == 2, !parts[0].isEmpty, ["yes", "no"].contains(parts[1]), behaviors[parts[0]] == nil else { throw DraftError.message("Use a unique behavior name = yes or no on each line.") }
                    behaviors[parts[0]] = .bool(parts[1] == "yes")
                }
                content[field.key] = .object(behaviors)
            } else if field.format == "measurements" || field.format == "times" {
                var items: [String: JSONValue] = [:]
                for line in lines(value) {
                    let parts = line.components(separatedBy: "=").map { $0.trimmingCharacters(in: .whitespaces) }
                    guard parts.count == 2, !parts[0].isEmpty, items[parts[0]] == nil else { throw DraftError.message("Use unique names and one = per line.") }
                    if field.format == "times" {
                        let clock = parts[1].split(separator: ":", omittingEmptySubsequences: false)
                        guard clock.count == 2, clock.allSatisfy({ $0.count == 2 && $0.allSatisfy({ $0.isASCII && $0.isNumber }) }), let hour = Int(clock[0]), let minute = Int(clock[1]), (0..<24).contains(hour), (0..<60).contains(minute) else { throw DraftError.message("Use a 24-hour time in HH:MM format.") }
                        items[parts[0]] = .number(Double(hour * 60 + minute))
                    } else {
                        guard let split = parts[1].firstIndex(where: { $0.isWhitespace }), let number = Double(parts[1][..<split]), number.isFinite else { throw DraftError.message("Each measurement needs a number and an explicit unit.") }
                        let unit = String(parts[1][split...]).trimmingCharacters(in: .whitespaces)
                        guard !unit.isEmpty else { throw DraftError.message("Enter a measurement unit.") }
                        items[parts[0]] = .object(["value": .number(number), "unit": .string(unit)])
                    }
                }
                content[field.key] = .object(items)
            } else if field.format == "lines" { content[field.key] = .array(lines(value).map(JSONValue.string)) }
            else { content[field.key] = .string(value) }
        }
        if kind == "food" {
            if let raw = values["_external_source"], let data = raw.data(using: .utf8) { content["external_source"] = try JSONDecoder().decode(JSONValue.self, from: data) }
            else { content["external_source"] = .null }
        }
        if kind == "nutrition" {
            if let raw = values["_origin"], let data = raw.data(using: .utf8) { content["origin"] = try JSONDecoder().decode(JSONValue.self, from: data) }
            else { content["origin"] = .null }
        }
        if kind == "meal_plan" {
            let recipeKeys = ["name", "servings", "ingredients", "instructions", "note"]
            var recipe: [String: JSONValue] = [:]
            for key in recipeKeys { recipe[key] = content.removeValue(forKey: key) }
            content["recipe"] = .object(recipe)
        }
        return .object(["kind": .string(kind), "content": .object(content)])
    }
    public static func decode(_ entry: JSONValue) -> [String: String] {
        var result: [String: String] = [:]
        if entry["kind"] == .string("food"), entry["content"]["external_source"] != .null { result["_external_source"] = (try? JSONEncoder().encode(entry["content"]["external_source"])).flatMap { String(data: $0, encoding: .utf8) } }
        for field in fields(entry["kind"].stringValue) {
            let value = entry["kind"] == .string("meal_plan") && !["planned_servings", "status"].contains(field.key) ? entry["content"]["recipe"][field.key] : entry["content"][field.key]
            switch field.format {
            case "datetime", "required-datetime": result[field.key] = value.numberValue.map { ISO8601DateFormatter().string(from: Date(timeIntervalSince1970: $0)) } ?? ""
            case "bool": result[field.key] = value.boolValue ? "true" : "false"
            case "integers": result[field.key] = value.arrayValue.compactMap(\.numberValue).map { String(Int($0)) }.joined(separator: "\n")
            case "ingredients": result[field.key] = (try? JSONEncoder().encode(value)).flatMap { String(data: $0, encoding: .utf8) } ?? "[]"
            case "nutrients": result[field.key] = value.objectValue.keys.sorted().map { "\($0) = \(value[$0].numberValue ?? 0)" }.joined(separator: "\n")
            case "prescriptions": result[field.key] = value.arrayValue.map { item in ["exercise", "equipment", "sets", "repetitions", "duration_seconds", "external_weight_kg", "rest_seconds"].map { item[$0].numberValue.map { String($0) } ?? item[$0].stringValue }.joined(separator: ", ") }.joined(separator: "\n")
            case "sets": result[field.key] = value.arrayValue.map { "\($0["exercise"].stringValue), \(Int($0["repetitions"].numberValue ?? 0)), \($0["external_weight_kg"].numberValue ?? 0)" }.joined(separator: "\n")
            case "behaviors": result[field.key] = value.objectValue.keys.sorted().map { "\($0) = \(value[$0].boolValue ? "yes" : "no")" }.joined(separator: "\n")
            case "measurements": result[field.key] = value.objectValue.keys.sorted().map { "\($0) = \(value[$0]["value"].numberValue ?? 0) \(value[$0]["unit"].stringValue)" }.joined(separator: "\n")
            case "times": result[field.key] = value.objectValue.keys.sorted().map { key in
                let minute = Int(value[key].numberValue ?? 0)
                return "\(key) = " + String(format: "%02d:%02d", minute / 60, minute % 60)
            }.joined(separator: "\n")
            case "lines": result[field.key] = value.arrayValue.map(\.stringValue).joined(separator: "\n")
            default: result[field.key] = value.numberValue.map { String($0) } ?? value.stringValue
            }
        }
        if entry["content"]["origin"] != .null, let data = try? JSONEncoder().encode(entry["content"]["origin"]) { result["_origin"] = String(data: data, encoding: .utf8) }
        return result
    }
    private static func lines(_ value: String) -> [String] { value.components(separatedBy: .newlines).map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty } }
}
public nonisolated enum DraftError: LocalizedError {
    case message(String)
    public var errorDescription: String? { if case .message(let text) = self { return text }; return nil }
}
