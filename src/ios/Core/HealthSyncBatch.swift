import Foundation

public nonisolated enum HealthSyncBatch {
    public static func requests(connectionID: String, recordType: String, coverageStatus: String, records: [JSONValue], maximumRecords: Int, maximumBytes: Int) throws -> [JSONValue] {
        func request(_ values: [JSONValue]) -> JSONValue {
            .object(["connection_id": .string(connectionID), "batch_id": .string(UUID().uuidString),
                     "record_type": .string(recordType), "coverage_status": .string(coverageStatus), "records": .array(values)])
        }
        guard maximumRecords > 0 else { throw APIError.invalidResponse }
        var batches: [JSONValue] = []
        var page: [JSONValue] = []
        for record in records {
            let exceedsBytes = try JSONEncoder().encode(request(page + [record])).count > maximumBytes
            if page.count == maximumRecords || exceedsBytes {
                guard !page.isEmpty else { throw APIError.status(413, "A Health record exceeds the server limit") }
                batches.append(request(page)); page = []
                guard try JSONEncoder().encode(request([record])).count <= maximumBytes else {
                    throw APIError.status(413, "A Health record exceeds the server limit")
                }
            }
            page.append(record)
        }
        if !page.isEmpty || batches.isEmpty { batches.append(request(page)) }
        return batches
    }
}
