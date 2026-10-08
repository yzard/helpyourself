import SwiftUI
import Speech
import AVFoundation
import Observation

struct VoiceQuestionButton: View {
    @Binding var question: String
    @State private var capture = VoiceQuestionCapture()
    @Environment(\.scenePhase) private var scenePhase
    var body: some View {
        VStack(alignment: .leading) {
            Button(capture.recording ? "Stop and transcribe" : "Dictate on this device") {
                if capture.recording { capture.transcribe() } else { Task { await capture.start() } }
            }.disabled(capture.processing)
            if capture.processing { ProgressView("Transcribing on this device") }
            Text("Up to 60 seconds. Review the text before submitting. Audio is temporary and is not uploaded.").font(.caption)
            if let failure = capture.failure { Text(failure).font(.caption).foregroundStyle(.secondary) }
        }.onChange(of: capture.transcript) { _, value in if !value.isEmpty { question = value } }
            .onChange(of: scenePhase) { _, phase in if phase == .background { capture.cancel() } }
            .onDisappear { capture.cancel() }
    }
}

@MainActor @Observable private final class VoiceQuestionCapture {
    var recording = false
    var processing = false
    var transcript = ""
    var failure: String?
    private var recorder: AVAudioRecorder?
    private var recognition: SFSpeechRecognitionTask?
    private var recognizer: SFSpeechRecognizer?
    private var audioURL: URL?
    private var limit: Task<Void, Never>?
    private var generation = UUID()
    private var audioActive = false
    func start() async {
        cancel(); failure = nil; transcript = ""
        let ticket = generation
        guard let recognizer = SFSpeechRecognizer(locale: .current), recognizer.isAvailable, recognizer.supportsOnDeviceRecognition else {
            failure = "On-device dictation is unavailable for the current language. Type your question."; return
        }
        processing = true
        let authorized = await withCheckedContinuation { continuation in
            SFSpeechRecognizer.requestAuthorization { state in continuation.resume(returning: state == .authorized) }
        }
        guard generation == ticket else { return }
        guard authorized else { processing = false; failure = "Speech permission was not granted."; return }
        let microphone = await AVAudioApplication.requestRecordPermission()
        guard generation == ticket else { return }
        guard microphone else { processing = false; failure = "Microphone permission was not granted."; return }
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .measurement, options: [.duckOthers])
            try session.setActive(true); audioActive = true
            let url = FileManager.default.temporaryDirectory.appendingPathComponent("helpyourself-question-\(UUID().uuidString).m4a")
            audioURL = url
            let recorder = try AVAudioRecorder(url: url, settings: [AVFormatIDKey: Int(kAudioFormatMPEG4AAC), AVSampleRateKey: 16000, AVNumberOfChannelsKey: 1])
            self.recorder = recorder
            guard recorder.record(forDuration: 60) else { throw VoiceCaptureError.recordingFailed }
            recording = true; processing = false
            limit = Task { [weak self] in
                try? await Task.sleep(for: .seconds(60))
                guard !Task.isCancelled, let self, self.generation == ticket else { return }
                self.transcribe()
            }
        } catch { cancel(); failure = error.localizedDescription }
    }
    func transcribe() {
        guard recording, let url = audioURL else { return }
        limit?.cancel(); limit = nil; recorder?.stop(); recorder = nil; recording = false; processing = true
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation); audioActive = false
        guard let recognizer = SFSpeechRecognizer(locale: .current), recognizer.isAvailable, recognizer.supportsOnDeviceRecognition else {
            cancel(); failure = "On-device dictation is unavailable. Type your question."; return
        }
        self.recognizer = recognizer
        let request = SFSpeechURLRecognitionRequest(url: url)
        request.requiresOnDeviceRecognition = true; request.shouldReportPartialResults = false
        let ticket = generation
        recognition = recognizer.recognitionTask(with: request) { [weak self] result, error in
            let text = result?.bestTranscription.formattedString
            let finished = result?.isFinal == true
            let message = error?.localizedDescription
            Task { @MainActor [weak self] in
                guard let self, self.generation == ticket else { return }
                if finished { self.cancel(); self.transcript = text ?? "" }
                else if let message { self.cancel(); self.failure = message }
            }
        }
        limit = Task { [weak self] in
            try? await Task.sleep(for: .seconds(60))
            guard !Task.isCancelled, let self, self.generation == ticket else { return }
            self.cancel(); self.failure = "Dictation timed out. Type your question or try again."
        }
    }
    func cancel() {
        generation = UUID(); limit?.cancel(); limit = nil
        recorder?.stop(); recorder = nil
        recognition?.cancel(); recognition = nil; recognizer = nil
        if let url = audioURL { try? FileManager.default.removeItem(at: url) }; audioURL = nil
        if audioActive { try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation) }; audioActive = false
        recording = false; processing = false
    }
}
private enum VoiceCaptureError: LocalizedError {
    case recordingFailed
    var errorDescription: String? { "The microphone could not start recording." }
}
