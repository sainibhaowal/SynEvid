import { TelemetryCollector } from '../analytics/telemetry';

export class OrphanCleaner {
    constructor(private collector: TelemetryCollector) {}

    public performCleanup(): void {
        this.collector.trackEvent("cleanup_initiated");
    }
}
