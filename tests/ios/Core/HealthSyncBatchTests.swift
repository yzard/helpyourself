import Foundation
import Testing
@testable import HelpYourselfCore

@Test func batchesFitTransportAndRetainEveryRawRecord() throws {
    let records = (0..<3).map { JSONValue.object(["record_id": .string("sample-\($0)"), "payload": .string(String(repeating: "x", count: 200))]) }
    let requests = try HealthSyncBatch.requests(connectionID: "connection", recordType: "raw_type", coverageStatus: "observed", records: records, maximumRecords: 500, maximumBytes: 600)
    #expect(requests.count > 1)
    #expect(requests.flatMap { $0["records"].arrayValue } == records)
    #expect(Set(requests.map { $0["batch_id"].stringValue }).count == requests.count)
    for request in requests { #expect(try JSONEncoder().encode(request).count <= 600) }
    #expect(throws: APIError.self) {
        try HealthSyncBatch.requests(connectionID: "connection", recordType: "raw_type", coverageStatus: "observed", records: records, maximumRecords: 500, maximumBytes: 100)
    }
}

@Test func batchesRespectCountLimitAndSendEmptyCoverage() throws {
    let records = (0..<501).map { JSONValue.object(["record_id": .string("sample-\($0)"), "payload": .object([:])]) }
    let requests = try HealthSyncBatch.requests(connectionID: "connection", recordType: "raw_type", coverageStatus: "observed", records: records, maximumRecords: 500, maximumBytes: 40 * 1024 * 1024)
    #expect(requests.map { $0["records"].arrayValue.count } == [500, 1])
    #expect(requests.flatMap { $0["records"].arrayValue } == records)
    let empty = try HealthSyncBatch.requests(connectionID: "connection", recordType: "raw_type", coverageStatus: "no_visible_samples", records: [], maximumRecords: 500, maximumBytes: 40 * 1024 * 1024)
    #expect(empty.count == 1)
    #expect(empty[0]["coverage_status"] == .string("no_visible_samples"))
    #expect(empty[0]["records"].arrayValue.isEmpty)
}
