import Foundation
@preconcurrency import HealthKit

@MainActor enum HealthSnapshots {
    static let characteristics: [HKCharacteristicTypeIdentifier] = [.biologicalSex, .bloodType, .dateOfBirth, .fitzpatrickSkinType, .wheelchairUse, .activityMoveMode]
    static var readTypes: Set<HKObjectType> {
        var result = Set(characteristics.compactMap { HKObjectType.characteristicType(forIdentifier: $0) as HKObjectType? })
        result.insert(HKObjectType.activitySummaryType())
        if let type = HKObjectType.documentType(forIdentifier: .CDA) { result.insert(type) }
        if #available(iOS 26.0, *) { result.insert(HKObjectType.userAnnotatedMedicationType()) }
        return result
    }

    struct Value: Sendable { let id: String; let payload: JSONValue }
    struct Checkpoint: Codable {
        let requests: [JSONValue]
        let records: [JSONValue]
    }

    static func synchronize(store: HKHealthStore, client: APIClient, archive: LocalArchive, connectionID: String, installationID: UUID) async throws -> [String] {
        var failed: [String] = []
        var kinds = characteristics.map(\.rawValue) + ["activity_summary", "cda_document"]
        if #available(iOS 26.0, *) { kinds.append("user_annotated_medications") }
        for kind in kinds {
            try Task.checkCancellation()
            let pendingName = "pending-snapshot-\(kind).json"
            let savedName = "snapshot-\(kind).json"
            if let pending = try archive.load(Checkpoint.self, name: pendingName) {
                for request in pending.requests { _ = try await client.post("health/sync", body: request) }
                try archive.save(pending.records, name: savedName)
                try archive.remove(name: pendingName)
            }
            let values: [Value]
            do { values = try await read(kind, store: store) }
            catch is CancellationError { throw CancellationError() }
            catch {
                let requests = try HealthSyncBatch.requests(connectionID: connectionID, recordType: kind, coverageStatus: "error", records: [], maximumRecords: 500, maximumBytes: 40 * 1024 * 1024)
                for request in requests { _ = try await client.post("health/sync", body: request) }
                failed.append(kind); continue
            }
            let previous = try archive.load([JSONValue].self, name: savedName) ?? []
            let now = Date().timeIntervalSince1970
            var records: [JSONValue] = []
            var changed: [JSONValue] = []
            for value in values {
                let old = previous.first { $0["record_id"].stringValue == value.id }
                if let old, old["payload"] == value.payload { records.append(old); continue }
                let record: JSONValue = .object(["record_id": .string(value.id), "source_id": .string("snapshot:\(installationID.uuidString)"),
                    "record_type": .string(kind), "start_at": .number(floor(now)), "end_at": .number(floor(now)),
                    "version": .number(max(floor(now * 1000), (old?["version"].numberValue ?? 0) + 1)), "deleted": .bool(false), "payload": value.payload])
                records.append(record); changed.append(record)
            }
            // Missing/hidden snapshots do not imply deletions. Keep previous visible values for retries.
            let visibleIDs = Set(records.map { $0["record_id"].stringValue })
            records += previous.filter { !visibleIDs.contains($0["record_id"].stringValue) }
            let requests = try HealthSyncBatch.requests(connectionID: connectionID, recordType: kind,
                coverageStatus: values.isEmpty ? "no_visible_samples" : "observed", records: changed, maximumRecords: 500, maximumBytes: 40 * 1024 * 1024)
            try archive.save(Checkpoint(requests: requests, records: records), name: pendingName)
            for request in requests { _ = try await client.post("health/sync", body: request) }
            try archive.save(records, name: savedName)
            try archive.remove(name: pendingName)
        }
        return failed
    }

    private static func read(_ kind: String, store: HKHealthStore) async throws -> [Value] {
        switch kind {
        case HKCharacteristicTypeIdentifier.biologicalSex.rawValue:
            return [Value(id: kind, payload: .object(["value": .number(Double(try store.biologicalSex().biologicalSex.rawValue))]))]
        case HKCharacteristicTypeIdentifier.bloodType.rawValue:
            return [Value(id: kind, payload: .object(["value": .number(Double(try store.bloodType().bloodType.rawValue))]))]
        case HKCharacteristicTypeIdentifier.fitzpatrickSkinType.rawValue:
            return [Value(id: kind, payload: .object(["value": .number(Double(try store.fitzpatrickSkinType().skinType.rawValue))]))]
        case HKCharacteristicTypeIdentifier.wheelchairUse.rawValue:
            return [Value(id: kind, payload: .object(["value": .number(Double(try store.wheelchairUse().wheelchairUse.rawValue))]))]
        case HKCharacteristicTypeIdentifier.activityMoveMode.rawValue:
            return [Value(id: kind, payload: .object(["value": .number(Double(try store.activityMoveMode().activityMoveMode.rawValue))]))]
        case HKCharacteristicTypeIdentifier.dateOfBirth.rawValue:
            let components = try store.dateOfBirthComponents()
            return [Value(id: kind, payload: .object(["date_components_archive": .string(try NSKeyedArchiver.archivedData(withRootObject: components as NSDateComponents, requiringSecureCoding: true).base64EncodedString())]))]
        case "activity_summary":
            let summaries = try await HKActivitySummaryQueryDescriptor(predicate: nil).result(for: store)
            let calendar = Calendar(identifier: .gregorian)
            return try summaries.map { summary in
                let date = summary.dateComponents(for: calendar)
                guard let year = date.year, let month = date.month, let day = date.day else { throw APIError.invalidResponse }
                let dayID = String(format: "%04d-%02d-%02d", year, month, day)
                return Value(id: dayID, payload: .object(["date": .string(dayID), "timezone": .string(calendar.timeZone.identifier), "raw_archive": try archive(summary)]))
            }
        case "cda_document":
            return try await HealthSeriesReader.documents(store: store).map { document in
                Value(id: document["record_id"].stringValue, payload: .object(["original_record": document]))
            }
        case "user_annotated_medications":
            if #available(iOS 26.0, *) {
                let medications = try await HKUserAnnotatedMedicationQueryDescriptor(predicate: nil, limit: nil).result(for: store)
                guard !medications.isEmpty else { return [] }
                return [Value(id: kind, payload: .object(["raw_medications": .array(try medications.map { try archive($0) })]))]
            }
            throw APIError.invalidResponse
        default: throw APIError.invalidResponse
        }
    }
    private static func archive(_ value: Any) throws -> JSONValue {
        .object(["format": .string("nskeyedarchiver-secure-v1"), "encoding": .string("base64"),
                 "data": .string(try NSKeyedArchiver.archivedData(withRootObject: value, requiringSecureCoding: true).base64EncodedString())])
    }
}
