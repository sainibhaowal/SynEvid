import { MessageEnvelope, EventHandler } from './types';

export class EventBus {
    private handlers: Map<string, EventHandler[]> = new Map();

    public subscribe(topic: string, handler: EventHandler): void {
        const list = this.handlers.get(topic) || [];
        list.push(handler);
        this.handlers.set(topic, list);
    }

    public publish(envelope: MessageEnvelope): void {
        const list = this.handlers.get(envelope.topic) || [];
        for (const h of list) {
            h.handle(envelope);
        }
    }
}
