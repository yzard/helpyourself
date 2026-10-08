import SwiftUI
import EventKit
import EventKitUI

struct CalendarPlanButton: View {
    let title: String
    let start: Date
    let durationMinutes: Double
    @State private var showing = false
    @State private var saved = false
    var body: some View {
        VStack(alignment: .leading) {
            Button(saved ? "Review another calendar copy" : "Review in Calendar") { showing = true }
            if saved { Text("A calendar copy was saved. Later changes to this plan do not update that copy.").font(.caption) }
        }.sheet(isPresented: $showing) {
            CalendarEventEditor(title: title, start: start, durationMinutes: durationMinutes) { action in
                if action == .saved { saved = true }
                showing = false
            }
        }
    }
}

private struct CalendarEventEditor: UIViewControllerRepresentable {
    let title: String
    let start: Date
    let durationMinutes: Double
    let completion: (EKEventEditViewAction) -> Void
    func makeCoordinator() -> Coordinator { Coordinator(completion: completion) }
    func makeUIViewController(context: Context) -> EKEventEditViewController {
        let store = EKEventStore()
        let event = EKEvent(eventStore: store)
        event.title = title; event.startDate = start
        event.endDate = start.addingTimeInterval(max(1, durationMinutes) * 60)
        event.notes = "Copy of a Helpyourself plan. Changes to the original plan do not update this calendar event."
        let controller = EKEventEditViewController()
        controller.eventStore = store; controller.event = event; controller.editViewDelegate = context.coordinator
        return controller
    }
    func updateUIViewController(_ controller: EKEventEditViewController, context: Context) {}
    final class Coordinator: NSObject, EKEventEditViewDelegate {
        private let completion: (EKEventEditViewAction) -> Void
        init(completion: @escaping (EKEventEditViewAction) -> Void) { self.completion = completion }
        func eventEditViewController(_ controller: EKEventEditViewController, didCompleteWith action: EKEventEditViewAction) { completion(action) }
    }
}
