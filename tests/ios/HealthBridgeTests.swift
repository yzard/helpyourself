import Testing
import Foundation
import HealthKit
@testable import Helpyourself

@MainActor @Test func healthDefinitionsUseUniqueTypesAndExplicitCoverage() {
    let bridge = HealthBridge()
    let kinds = bridge.definitions.map(\.kind)
    #expect(Set(kinds).count == kinds.count)
    #expect(kinds.contains("sleep"))
    #expect(kinds.contains("workout"))
    #expect(kinds.contains("hrv_sdnn"))
    #expect(!bridge.isSyncing)
    #expect(HealthBridge.bulkReadTypes(supportsClinical: false).allSatisfy { !$0.requiresPerObjectAuthorization() && !($0 is HKCorrelationType) && !($0 is HKClinicalType) && $0.identifier != "HKMedicationDoseEventTypeIdentifierMedicationDoseEvent" })
    #expect(kinds.contains("electrocardiogram"))
    #expect(kinds.contains("workout_route"))
    #expect(Set(bridge.definitions.map { $0.type.identifier }).count == bridge.definitions.count)
    for definition in bridge.definitions {
        if let quantity = definition.type as? HKQuantityType, let unit = definition.unit {
            #expect(quantity.is(compatibleWith: HKUnit(from: unit)), "Invalid archive unit for \(definition.kind)")
        }
    }
}

@Test func rawHealthSampleCanBeRestoredWithoutLosingMetadata() throws {
    let type = try #require(HKObjectType.quantityType(forIdentifier: .bodyMass))
    let sample = HKQuantitySample(type: type, quantity: HKQuantity(unit: .pound(), doubleValue: 150), start: Date(timeIntervalSince1970: 100.25), end: Date(timeIntervalSince1970: 101.75), metadata: ["raw_flag": true, "original_note": "original"])
    let record = try HealthBridge.encode(sample, kind: "body_mass", unit: "kg")
    let bytes = try #require(Data(base64Encoded: record["payload"]["raw_archive"]["data"].stringValue))
    let restored = try #require(try NSKeyedUnarchiver.unarchivedObject(ofClass: HKQuantitySample.self, from: bytes))
    #expect(restored.uuid == sample.uuid)
    #expect(restored.startDate == sample.startDate)
    #expect(restored.quantity.doubleValue(for: .pound()) == 150)
    #expect(restored.metadata?["raw_flag"] as? Bool == true)
}

@Test func glucoseWritePreservesCollectionTimeAndRetryIdentity() throws {
    let observation: JSONValue = .object(["status": .string("confirmed"), "observation_id": .string("reviewed-1"), "revision": .number(2),
        "payload": .object(["metric_id": .string("glucose"), "raw_result": .string("5.5"), "raw_unit": .string("mmol/L")])])
    let server = URL(string: "https://example.com")!
    let time = Date(timeIntervalSince1970: 1700000000)
    let first = try HealthBridge.reviewedGlucoseSample(observation, server: server, userID: "one", sampledAt: time)
    let retry = try HealthBridge.reviewedGlucoseSample(observation, server: server, userID: "one", sampledAt: time)
    let other = try HealthBridge.reviewedGlucoseSample(observation, server: server, userID: "two", sampledAt: time)
    #expect(first.startDate == time && first.endDate == time)
    #expect(abs(first.quantity.doubleValue(for: HKUnit(from: "mg/dL")) - 99.085734) < 0.001)
    #expect(first.metadata?[HKMetadataKeySyncVersion] as? Int == 2)
    #expect(first.metadata?[HKMetadataKeySyncIdentifier] as? String == retry.metadata?[HKMetadataKeySyncIdentifier] as? String)
    #expect(first.metadata?[HKMetadataKeySyncIdentifier] as? String != other.metadata?[HKMetadataKeySyncIdentifier] as? String)
    for (key, invalid) in [("raw_result", "<5"), ("raw_result", "nan"), ("raw_result", "-1"), ("raw_unit", "%")] {
        var bad = observation; bad["payload"][key] = .string(invalid)
        #expect(throws: APIError.self) { try HealthBridge.reviewedGlucoseSample(bad, server: server, userID: "one", sampledAt: time) }
    }
    var pending = observation; pending["status"] = .string("pending")
    #expect(throws: APIError.self) { try HealthBridge.reviewedGlucoseSample(pending, server: server, userID: "one", sampledAt: time) }
}

@Test func categoriesCorrelationsAndWorkoutsRetainEvidence() throws {
    let start = Date(timeIntervalSince1970: 1700000000)
    let end = start.addingTimeInterval(3600)
    let sleep = HKCategorySample(type: HKObjectType.categoryType(forIdentifier: .sleepAnalysis)!, value: HKCategoryValueSleepAnalysis.asleepCore.rawValue, start: start, end: end)
    let encoded = try HealthBridge.encode(sleep, kind: "sleep", unit: nil)
    #expect(encoded["payload"]["category"].numberValue == Double(HKCategoryValueSleepAnalysis.asleepCore.rawValue))
    let systolic = HKQuantitySample(type: HKObjectType.quantityType(forIdentifier: .bloodPressureSystolic)!, quantity: HKQuantity(unit: .millimeterOfMercury(), doubleValue: 120), start: start, end: start)
    let diastolic = HKQuantitySample(type: HKObjectType.quantityType(forIdentifier: .bloodPressureDiastolic)!, quantity: HKQuantity(unit: .millimeterOfMercury(), doubleValue: 80), start: start, end: start)
    let pressure = HKCorrelation(type: HKObjectType.correlationType(forIdentifier: .bloodPressure)!, start: start, end: start, objects: [systolic, diastolic])
    #expect(try HealthBridge.encode(pressure, kind: "blood_pressure", unit: nil)["payload"]["related_sample_ids"].arrayValue.count == 2)
    let workout = HKWorkout(activityType: .walking, start: start, end: end)
    #expect(try HealthBridge.encode(workout, kind: "workout", unit: nil)["payload"]["duration_seconds"].numberValue == 3600)
}

@MainActor @Test func healthSyncUsesRealSimulatorQueriesAndDurableCheckpoints() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    let archive = try LocalArchive(directory: root)
    defer { try? FileManager.default.removeItem(at: root) }
    try archive.save(true, name: "health-enabled.json")
    let bridge = HealthBridge()
    let client = APIClient(server: URL(string: "http://localhost:18765")!, token: "synthetic-token")
    try await bridge.synchronize(client: client, archive: archive, requestAccess: false)
    #expect(!bridge.isSyncing)
    #expect(!bridge.status.contains("paused"))
    #expect(try archive.load(UUID.self, name: "installation.json") != nil)
    // A denied/empty query is still a completed checkpoint; never infer read authorization.
    let files = try FileManager.default.contentsOfDirectory(atPath: root.path)
    #expect(!files.contains { $0.hasPrefix("pending-") })
    #expect(files.contains { $0.hasPrefix("anchor-") })
}

@MainActor @Test func healthSyncFailureRetainsPendingBatchBeforeAdvancingAnchor() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    let archive = try LocalArchive(directory: root)
    defer { try? FileManager.default.removeItem(at: root) }
    try archive.save(true, name: "health-enabled.json")
    let bridge = HealthBridge()
    let kind = try #require(bridge.definitions.first?.kind)
    let anchor = try NSKeyedArchiver.archivedData(withRootObject: HKQueryAnchor(fromValue: 0), requiringSecureCoding: true)
    let requests = try HealthSyncBatch.requests(connectionID: "synthetic-connection", recordType: kind, coverageStatus: "no_visible_samples", records: [], maximumRecords: 500, maximumBytes: 40 * 1024 * 1024)
    try archive.save(SyncCheckpoint(requests: requests, nextAnchor: anchor), name: "pending-\(kind).json")
    let rejected = APIClient(server: URL(string: "http://127.0.0.1:18765")!, token: "reject-sync")
    do { try await bridge.synchronize(client: rejected, archive: archive, requestAccess: false); Issue.record("Rejected batch was accepted") }
    catch { #expect(bridge.status.contains("paused")) }
    #expect(!bridge.isSyncing)
    #expect(try archive.load(SyncCheckpoint.self, name: "pending-\(kind).json")?.requests == requests)
    #expect(try archive.load(Data.self, name: "anchor-\(kind).json") == nil)
    let accepted = APIClient(server: URL(string: "http://127.0.0.1:18765")!, token: "synthetic-token")
    try await bridge.synchronize(client: accepted, archive: archive, requestAccess: false)
    #expect(try archive.load(SyncCheckpoint.self, name: "pending-\(kind).json") == nil)
    #expect(try archive.load(Data.self, name: "anchor-\(kind).json") != nil)
}
