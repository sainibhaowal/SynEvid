export interface MessageEnvelope<T = any> {
    id: string;
    topic: string;
    payload: T;
    timestamp: number;
}

export interface EventHandler<T = any> {
    handle(event: MessageEnvelope<T>): Promise<void> | void;
}
