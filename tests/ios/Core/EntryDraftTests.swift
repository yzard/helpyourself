import Testing
@testable import HelpYourselfCore

@Test func entryDraftPreservesUnknownsAndRoundTripsSets() throws {
    let entry = try EntryDraft.encode(kind: "training", values: ["activity": "Strength", "duration_minutes": "45", "ended_at": "2026-01-01T01:00:00Z", "paused_minutes": "0", "sets": "Squat, 5, 60\nPress, 3, 20"])
    #expect(entry["content"]["rpe_cr10"] == .null)
    #expect(entry["content"]["sets"].arrayValue.count == 2)
    #expect(try EntryDraft.encode(kind: "training", values: EntryDraft.decode(entry)) == entry)
    #expect(throws: (any Error).self) { try EntryDraft.encode(kind: "training", values: ["activity": "Run", "duration_minutes": "NaN"]) }
    #expect(throws: (any Error).self) { try EntryDraft.encode(kind: "journal", values: ["behaviors": "caffeine = yes\ncaffeine = no"]) }
    let food = try EntryDraft.encode(kind: "nutrition", values: ["food": "Water", "meal": "Drink", "water_ml": "250"])
    #expect(food["content"]["energy_kcal"] == .null)
    #expect(food["content"]["water_ml"].numberValue == 250)
}

@Test func journalDraftPreservesNumericUnitsAndClockTimes() throws {
    let entry = try EntryDraft.encode(kind: "journal", values: ["behaviors": "alcohol = no", "measurements": "caffeine = 120 mg", "times": "bedtime = 23:40"])
    #expect(entry["content"]["measurements"]["caffeine"]["unit"] == .string("mg"))
    #expect(entry["content"]["times"]["bedtime"].numberValue == 1420)
    #expect(try EntryDraft.encode(kind: "journal", values: EntryDraft.decode(entry)) == entry)
    #expect(throws: (any Error).self) { try EntryDraft.encode(kind: "journal", values: ["times": "bedtime = 24:00"]) }
    #expect(throws: (any Error).self) { try EntryDraft.encode(kind: "journal", values: ["measurements": "caffeine = NaN mg"]) }
}
