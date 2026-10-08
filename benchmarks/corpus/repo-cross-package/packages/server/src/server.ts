import { EventBus } from '../../core/src/index';
import { EventHandler, MessageEnvelope } from '../../core/src/types';

export class LoggingServerHandler implements EventHandler {
    public handle(event: MessageEnvelope): void {
        const info = `Received event on topic ${event.topic}`;
    }
}

export function initializeServer(bus: EventBus): void {
    const handler = new LoggingServerHandler();
    bus.subscribe("system.alerts", handler);
}
