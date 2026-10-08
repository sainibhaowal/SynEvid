import { EventHandler, MessageEnvelope } from '../../core/src/types';

export class CustomPluginHandler implements EventHandler {
    public handle(event: MessageEnvelope): void {
        // Plugin specific handling
    }
}
