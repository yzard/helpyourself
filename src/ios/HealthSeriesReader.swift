import Foundation
import CoreLocation
@preconcurrency import HealthKit

/// HealthKit invokes series callbacks repeatedly. The lock owns the buffer and
/// continuation so completion, errors and cancellation can only resume once.
private nonisolated final class HealthSeriesOperation: @unchecked Sendable {
    private let lock = NSLock()
    private let store: HKHealthStore
    private var query: HKQuery?
    private var continuation: CheckedContinuation<[JSONValue], any Error>?
    private var rows: [JSONValue] = []
    private var byteCount = 0
    private var finished = false

    init(store: HKHealthStore) { self.store = store }
    func start(_ query: HKQuery, continuation: CheckedContinuation<[JSONValue], any Error>) {
        lock.lock()
        if finished { lock.unlock(); continuation.resume(throwing: CancellationError()); return }
        self.query = query; self.continuation = continuation
        store.execute(query)
        lock.unlock()
    }
    func receive(_ values: [JSONValue], done: Bool, error: (any Error)?) {
        lock.lock()
        guard !finished else { lock.unlock(); return }
        var failure = error
        if failure == nil {
            do {
                byteCount += try JSONEncoder().encode(values).count
                if byteCount > 24 * 1024 * 1024 { failure = APIError.status(413, "Health series exceeds the archive limit") }
                else { rows.append(contentsOf: values) }
            } catch { failure = error }
        }
        guard done || failure != nil else { lock.unlock(); return }
        finished = true
        let continuation = self.continuation; self.continuation = nil
        let query = self.query; self.query = nil
        let result = rows; rows = []
        lock.unlock()
        if let query { store.stop(query) }
        if let failure { continuation?.resume(throwing: failure) }
        else { continuation?.resume(returning: result) }
    }
    func cancel() { receive([], done: true, error: CancellationError()) }
}

@MainActor enum HealthSeriesReader {
    static func documents(store: HKHealthStore) async throws -> [JSONValue] {
        guard let type = HKObjectType.documentType(forIdentifier: .CDA) else { throw APIError.invalidResponse }
        let operation = HealthSeriesOperation(store: store)
        return try await execute(operation) { continuation in
            let query = HKDocumentQuery(documentType: type, predicate: nil, limit: HKObjectQueryNoLimit, sortDescriptors: nil, includeDocumentData: true) { _, samples, done, error in
                do {
                    let rows = try (samples ?? []).map { try HealthBridge.encode($0, kind: "cda_document", unit: nil) }
                    operation.receive(rows, done: done, error: error)
                } catch { operation.receive([], done: true, error: error) }
            }
            operation.start(query, continuation: continuation)
        }
    }

    static func read(_ sample: HKSample, store: HKHealthStore) async throws -> [JSONValue]? {
        let operation = HealthSeriesOperation(store: store)
        if let ecg = sample as? HKElectrocardiogram {
            return try await execute(operation) { continuation in
                let query = HKElectrocardiogramQuery(ecg) { _, result in
                    switch result {
                    case .measurement(let measurement):
                        var row: [String: JSONValue] = ["time_since_start": .number(measurement.timeSinceSampleStart)]
                        if let quantity = measurement.quantity(for: .appleWatchSimilarToLeadI) {
                            row["lead_i_volts"] = .number(quantity.doubleValue(for: .volt()))
                        }
                        operation.receive([.object(row)], done: false, error: nil)
                    case .done: operation.receive([], done: true, error: nil)
                    case .error(let error): operation.receive([], done: true, error: error)
                    @unknown default: operation.receive([], done: true, error: APIError.invalidResponse)
                    }
                }
                operation.start(query, continuation: continuation)
            }
        }
        if let series = sample as? HKHeartbeatSeriesSample {
            return try await execute(operation) { continuation in
                let query = HKHeartbeatSeriesQuery(heartbeatSeries: series) { _, time, gap, done, error in
                    let values: [JSONValue] = error == nil ? [.object(["time_since_start": .number(time), "preceded_by_gap": .bool(gap)])] : []
                    operation.receive(values, done: done, error: error)
                }
                operation.start(query, continuation: continuation)
            }
        }
        if let route = sample as? HKWorkoutRoute {
            return try await execute(operation) { continuation in
                let query = HKWorkoutRouteQuery(route: route) { _, locations, done, error in
                    do {
                    let rows = try (locations ?? []).map { location in
                        JSONValue.object(["latitude": .number(location.coordinate.latitude), "longitude": .number(location.coordinate.longitude),
                                          "altitude": .number(location.altitude), "timestamp": .number(location.timestamp.timeIntervalSince1970),
                                          "horizontal_accuracy": .number(location.horizontalAccuracy), "vertical_accuracy": .number(location.verticalAccuracy),
                                          "speed": .number(location.speed), "speed_accuracy": .number(location.speedAccuracy),
                                          "course": .number(location.course), "course_accuracy": .number(location.courseAccuracy),
                                          "raw_location": .string(try NSKeyedArchiver.archivedData(withRootObject: location, requiringSecureCoding: true).base64EncodedString())])
                    }
                    operation.receive(rows, done: done, error: error)
                    } catch { operation.receive([], done: true, error: error) }
                }
                operation.start(query, continuation: continuation)
            }
        }
        if let quantity = sample as? HKQuantitySample, quantity.count > 1 {
            return try await execute(operation) { continuation in
                let query = HKQuantitySeriesSampleQuery(quantityType: quantity.quantityType, predicate: HKQuery.predicateForObject(with: quantity.uuid)) { _, value, interval, _, done, error in
                    var values: [JSONValue] = []
                    if let value, let interval {
                        do { values = [.object(["quantity": .string(value.description), "start_at": .number(interval.start.timeIntervalSince1970), "end_at": .number(interval.end.timeIntervalSince1970),
                            "raw_quantity": .string(try NSKeyedArchiver.archivedData(withRootObject: value, requiringSecureCoding: true).base64EncodedString())])] }
                        catch { operation.receive([], done: true, error: error); return }
                    }
                    operation.receive(values, done: done, error: error)
                }
                operation.start(query, continuation: continuation)
            }
        }
        return nil
    }

    private static func execute(_ operation: HealthSeriesOperation, start: (CheckedContinuation<[JSONValue], any Error>) -> Void) async throws -> [JSONValue] {
        try await withTaskCancellationHandler {
            try Task.checkCancellation()
            return try await withCheckedThrowingContinuation { continuation in start(continuation) }
        } onCancel: { operation.cancel() }
    }
}
