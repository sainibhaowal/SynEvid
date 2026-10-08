import { LiveDataPipeline } from '../active/livePipeline';

export class TelemetryCollector {
    constructor(private pipeline: LiveDataPipeline) {}

    public trackEvent(eventName: string): void {
        const processed = this.pipeline.processMessage(eventName);
    }
}
