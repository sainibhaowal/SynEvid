import { EventBus } from '../../core/src/index';
import { MessageEnvelope } from '../../core/src/types';
import { serializePayload } from '../../utils/src/helpers';

export class AppClient {
    constructor(private bus: EventBus) {}

    public sendNotification(topic: string, data: any): void {
        const payloadStr = serializePayload(data);
        const envelope: MessageEnvelope = {
            id: "msg_1",
            topic,
            payload: payloadStr,
            timestamp: Date.now(),
        };
        this.bus.publish(envelope);
    }
}
